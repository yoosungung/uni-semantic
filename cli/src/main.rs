//! `slg` — Semantic Layer Generator CLI.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use slg::{convert_pbip_to_lookml, schema_diff_paths};

#[derive(Parser, Debug)]
#[command(name = "slg", about = "Semantic Layer Generator (Hub-and-Spoke CLI)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Convert source metadata → target via Universal Semantic IR.
    Convert {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        /// Input path (BIM JSON / dir with model.bim for pbip).
        input: PathBuf,
        /// Optional output file (default: stdout).
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
    /// Compare two sources at IR fingerprint level (CLI only; no GUI).
    SchemaDiff {
        left: PathBuf,
        right: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("slg: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), slg::Error> {
    match cli.command {
        Commands::Convert {
            from,
            to,
            input,
            output,
        } => {
            let from = from.to_ascii_lowercase();
            let to = to.to_ascii_lowercase();
            if from != "pbip" || to != "lookml" {
                return Err(slg::Error::Msg(format!(
                    "unsupported convert {from}→{to}; MVP supports --from pbip --to lookml"
                )));
            }
            let lookml = convert_pbip_to_lookml(&input)?;
            if let Some(path) = output {
                std::fs::write(&path, lookml.as_bytes())?;
            } else {
                print!("{lookml}");
            }
            Ok(())
        }
        Commands::SchemaDiff { left, right } => {
            let diff = schema_diff_paths(&left, &right)?;
            print!("{}", diff.render());
            Ok(())
        }
    }
}
