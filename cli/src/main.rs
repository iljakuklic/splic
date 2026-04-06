use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use splic_compiler::{checker, lexer, parser, staging};
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

    // Names arena: owns all identifier strings for the lifetime of the pipeline.
    let names_arena = bumpalo::Bump::new();

    // Parse into src_arena; the arena lives until elaboration finishes
    // consuming the AST, then is dropped.
    let ast_arena = bumpalo::Bump::new();
    let lexer = lexer::Lexer::new(&source, &names_arena);
    let mut parser = parser::Parser::new(lexer, &ast_arena);
    let program = parser.parse_program().context("failed to parse program")?;

    // Elaborate/typecheck into core_arena; the arena lives until staging
    // finishes consuming the core IR, then is dropped.
    let core_arena = bumpalo::Bump::new();
    let core_program =
        checker::elaborate_program(&core_arena, &program).context("failed to elaborate program")?;
    drop(ast_arena);

    // Unstage into out_arena; core_arena is no longer needed after this.
    let staged_arena = bumpalo::Bump::new();
    let staged = staging::unstage_program(&staged_arena, &core_program)
        .context("failed to stage program")?;
    drop(core_arena);

    // Print the staged result, then out_arena is dropped at end of scope.
    println!("{staged}");

    Ok(())
}
