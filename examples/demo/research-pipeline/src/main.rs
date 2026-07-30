use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "research-pipeline", about = "Research Pipeline demo scaffold")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        #[arg(long)]
        question: String,
        #[arg(long)]
        materials: String,
        #[arg(long)]
        fault: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            question,
            materials,
            fault,
        } => {
            println!("Research Pipeline scaffold: runtime behavior is added in issues 002–004.");
            println!("question: {question}");
            println!("materials: {materials}");
            println!("fault scenario requested: {fault}");
        }
    }
    Ok(())
}
