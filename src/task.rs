use crate::util::stream_paginated;
use anyhow::{Error, Ok, Result};
use aws_config::SdkConfig;
use comfy_table::Table;
use futures::{StreamExt, pin_mut};

#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(subcommand)]
    operation: Operation,

    #[clap(long)]
    cluster: String,
}

impl Args {
    pub async fn run(self, config: SdkConfig) -> Result<()> {
        let client = aws_sdk_ecs::Client::new(&config);
        match self.operation {
            Operation::List => {
                let s = stream_paginated(
                    client.clone(),
                    self.cluster,
                    |client, cluster, token| async move {
                        let resp = client
                            .list_tasks()
                            .cluster(&cluster)
                            .set_next_token(token)
                            .send()
                            .await?;

                        let tasks = client
                            .describe_tasks()
                            .cluster(&cluster)
                            .set_tasks(resp.task_arns)
                            .send()
                            .await?;

                        Ok((tasks, cluster, resp.next_token))
                    },
                );

                pin_mut!(s);

                let mut table = Table::new();
                table.set_header(vec!["Task Id", "Group", "AZ", "Cpu", "Memory"]);

                while let Some(describe) = s.next().await.transpose()? {
                    for task in describe.tasks.unwrap_or_default() {
                        let task_id: String = task
                            .task_arn
                            .and_then(|x| Some(x.split('/').last()?.to_string()))
                            .unwrap_or_default();

                        table.add_row(vec![
                            task_id,
                            task.group.unwrap_or_default(),
                            task.availability_zone.unwrap_or_default(),
                            task.cpu.unwrap_or_default(),
                            task.memory.unwrap_or_default(),
                        ]);
                    }
                }

                println!("{table}");
            }
            Operation::Get(args) => match args.op {
                GetOp::Containers => {
                    println!("{}", args.task);

                    let resp = client
                        .describe_tasks()
                        .cluster(self.cluster)
                        .tasks(args.task)
                        .send()
                        .await?;

                    let mut table = Table::new();
                    table.set_header(vec!["Task Id", "AZ", "Cpu", "Memory", "Containers"]);

                    let task = resp
                        .tasks
                        .into_iter()
                        .flatten()
                        .next()
                        .ok_or_else(|| Error::msg("missing task"))?;

                    let mut table = Table::new();
                    table.set_header(vec!["Name", "ID", "Image", "Exit Code"]);

                    for container in task.containers.into_iter().flatten() {
                        let exit_code = container
                            .exit_code
                            .map(|x| x.to_string())
                            .unwrap_or_default();

                        table.add_row(vec![
                            container.name.unwrap_or_default(),
                            container.runtime_id.unwrap_or_default(),
                            container.image.unwrap_or_default(),
                            exit_code,
                        ]);
                    }
                    println!("{table}");
                }
            },
            Operation::PortForward(args) => {
                if args.port.is_empty() {
                    return Err(Error::msg("no port specified"));
                }

                let resp = client
                    .describe_tasks()
                    .cluster(&self.cluster)
                    .tasks(&args.task)
                    .send()
                    .await?;

                let tasks = resp.tasks.unwrap_or_default();
                if tasks.is_empty() {
                    return Err(Error::msg("Task not found"));
                }
                let task = tasks.into_iter().next().unwrap();
                let containers = task.containers.unwrap_or_default();

                let runtime_id = match containers.iter().find(|x| x.exit_code.is_none()) {
                    Some(container) => container
                        .runtime_id
                        .clone()
                        .ok_or_else(|| Error::msg("missing runtime id"))?,
                    None => return Err(Error::msg("no running container")),
                };

                let target = format!("ecs:{}_{}_{}", self.cluster, args.task, runtime_id);

                println!("Forwarding to {target}");

                let mut children = Vec::with_capacity(args.port.len());
                for port in args.port {
                    let (from, to) = port
                        .split_once(':')
                        .ok_or_else(|| Error::msg("invalid port"))?;

                    let child = tokio::process::Command::new("aws")
                        .arg("ssm")
                        .arg("start-session")
                        .arg("--target")
                        .arg(&target)
                        .arg("--document-name")
                        .arg("AWS-StartPortForwardingSessionToRemoteHost")
                        .arg("--parameters")
                        .arg(format!(
                            "{{\"portNumber\":[\"{to}\"], \"localPortNumber\":[\"{from}\"] }}"
                        ))
                        .spawn()?;

                    children.push(child);
                }

                tokio::signal::ctrl_c().await?;

                println!("Shutting down");

                for mut child in children {
                    child.kill().await?;
                }
            }
            Operation::Exec(args) => {
                let resp = client
                    .describe_tasks()
                    .cluster(&self.cluster)
                    .tasks(&args.task)
                    .send()
                    .await?;

                let tasks = resp.tasks.unwrap_or_default();
                if tasks.is_empty() {
                    return Err(Error::msg("Task not found"));
                }
                let task = tasks.into_iter().next().unwrap();
                let containers = task.containers.unwrap_or_default();

                let runtime_id = containers
                    .iter()
                    .find(|x| x.name.as_deref().unwrap_or_default() == &args.container)
                    .ok_or_else(|| Error::msg("container not found"))?
                    .runtime_id
                    .as_deref()
                    .ok_or_else(|| Error::msg("missing runtime id"))?;

                let target = format!("ecs:{}_{}_{}", self.cluster, args.task, runtime_id);

                println!("Exec target: {target}");

                let mut child = tokio::process::Command::new("aws")
                    .arg("ssm")
                    .arg("start-session")
                    .arg("--target")
                    .arg(&target)
                    .spawn()?;

                // We supress CTRL+C as it can result in leaking the SSM session
                loop {
                    tokio::select! {
                        _ = child.wait() => {
                            println!("Session Finished");
                            break;
                        }
                        _ = tokio::signal::ctrl_c() => println!("\nExit session using CTRL-D"),
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(clap::Subcommand, Debug)]
enum Operation {
    List,
    Get(GetArgs),
    PortForward(PortForwardArgs),
    Exec(ExecArgs),
}

#[derive(clap::Args, Debug)]
struct GetArgs {
    #[clap(long)]
    task: String,

    #[command(subcommand)]
    op: GetOp,
}

#[derive(clap::Subcommand, Debug)]
enum GetOp {
    Containers,
}

#[derive(clap::Args, Debug)]
struct PortForwardArgs {
    #[clap(long)]
    task: String,

    #[clap(long)]
    port: Vec<String>,
}

#[derive(clap::Args, Debug)]
struct ExecArgs {
    #[clap(long)]
    task: String,

    #[clap(long)]
    container: String,
}
