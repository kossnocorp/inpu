use clap::{Parser, Subcommand};
use std::path::PathBuf;
use weight::weight_command;

mod weight;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Weights a file
    Weight {
        /// The file to watch
        #[arg(short, long)]
        path: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Weight { path }) => weight_command(path),
        None => Ok(()),
    }
}
