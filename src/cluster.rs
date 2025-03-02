use crate::util::stream_paginated;
use anyhow::{Ok, Result};
use aws_config::SdkConfig;
use comfy_table::Table;
use futures::{StreamExt, pin_mut};
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(subcommand)]
    operation: Operation,
}

impl Args {
    pub async fn run(self, config: SdkConfig) -> Result<()> {
        let client = aws_sdk_ecs::Client::new(&config);
        match self.operation {
            Operation::List => {
                let s = stream_paginated(client.clone(), (), |client, _, token| async move {
                    let resp = client.list_clusters().set_next_token(token).send().await?;
                    let clusters = client
                        .describe_clusters()
                        .set_clusters(resp.cluster_arns)
                        .send()
                        .await?;
                    Ok((clusters, (), resp.next_token))
                });

                pin_mut!(s);

                let mut table = Table::new();
                table.set_header(vec!["Arn", "Name"]);

                while let Some(describe) = s.next().await.transpose()? {
                    for cluster in describe.clusters.unwrap_or_default() {
                        table.add_row(vec![
                            cluster.cluster_arn.unwrap_or_default(),
                            cluster.cluster_name.unwrap_or_default(),
                        ]);
                    }
                }

                println!("{table}");
                Ok(())
            }
        }
    }
}

#[derive(clap::Subcommand, Debug)]
enum Operation {
    List,
}
