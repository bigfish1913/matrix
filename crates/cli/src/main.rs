//! Matrix CLI - Command-line interface for the Agent Orchestrator

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "matrix")]
#[command(author, version, about = "Long-Running Agent Orchestrator", long_about = None)]
struct Args {
    /// Project goal description
    goal: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _args = Args::parse();
    println!("Matrix CLI - stub");
    Ok(())
}