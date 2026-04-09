use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "splic")]
#[command(about = "Splic compiler CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Stage a Splic source file, printing the object-level program
    Stage {
        /// Path to the Splic source file
        file: PathBuf,
    },
    /// Compile a Splic source file to a target binary
    Compile {
        /// Path to the Splic source file
        file: PathBuf,
        /// Compilation target
        #[arg(long, short)]
        target: CompileTarget,
        /// Output file path
        #[arg(long, short)]
        output: PathBuf,
    },
}

#[derive(Clone, ValueEnum)]
enum CompileTarget {
    Wasm,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Stage { file } => {
            let source = read_file(&file)?;
            let arena = bumpalo::Bump::new();
            let program = splic_driver::stage(&source, &arena)?;
            println!("{program}");
        }
        Commands::Compile {
            file,
            target,
            output,
        } => {
            let source = read_file(&file)?;
            let driver_target = match target {
                CompileTarget::Wasm => splic_driver::Target::Wasm,
            };
            let bytes = splic_driver::compile(&source, driver_target)?;
            std::fs::write(&output, bytes)
                .with_context(|| format!("failed to write output: {}", output.display()))?;
        }
    }
    Ok(())
}

fn read_file(path: &PathBuf) -> Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path.display()))
}
