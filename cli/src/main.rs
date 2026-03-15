use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use splic_compiler::{checker, eval, lexer, parser};
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
    /// Stage a Splic source file
    Stage {
        /// Path to the Splic source file
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Stage { file } => stage(&file)?,
    }

    Ok(())
}

fn stage(file: &PathBuf) -> Result<()> {
    // Read the file
    let source = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read file: {}", file.display()))?;

    // Create arenas for memory allocation
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // Lex
    let lexer = lexer::Lexer::new(&source);

    // Parse
    let mut parser = parser::Parser::new(lexer, &src_arena);
    let program = parser.parse_program().context("failed to parse program")?;

    // Elaborate/Typecheck
    let core_program =
        checker::elaborate_program(&core_arena, &program).context("failed to elaborate program")?;

    // Unstage
    let staged =
        eval::unstage_program(&core_arena, &core_program).context("failed to stage program")?;

    // Print the staged result
    println!("{staged}");

    Ok(())
}
