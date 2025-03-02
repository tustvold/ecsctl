mod cluster;
mod task;
mod util;

use anyhow::Result;
use aws_config::{ConfigLoader, Region};
use aws_sdk_ecs::config::BehaviorVersion;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "opfyx")]
#[command(bin_name = "opfyx")]
struct Args {
    #[command(subcommand)]
    op: Op,

    #[clap(long)]
    region: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Op {
    Cluster(cluster::Args),
    Task(task::Args),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut loader = ConfigLoader::default().behavior_version(BehaviorVersion::latest());
    if let Some(x) = args.region {
        loader = loader.region(Region::new(x));
    }
    let config = loader.load().await;

    match args.op {
        Op::Cluster(args) => args.run(config).await,
        Op::Task(args) => args.run(config).await,
    }
}
