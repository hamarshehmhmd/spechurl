//! Command-line interface for spechurl.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use spechurl::{check_suite, generate_suite, write_suite};

const LONG_ABOUT: &str = "\
spechurl converts an OpenAPI 3.0 or 3.1 document into a directory of Hurl
contract tests (one file per operation) and can check that a committed suite
still matches the spec.";

#[derive(Debug, Parser)]
#[command(
    name = "spechurl",
    version,
    about = "Convert OpenAPI 3.x specs into Hurl HTTP contract test suites",
    long_about = LONG_ABOUT,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate a Hurl contract suite from an OpenAPI 3.x document.
    Generate {
        /// Path to an OpenAPI 3.0 or 3.1 document (YAML or JSON).
        spec: PathBuf,
        /// Directory to write `.hurl` files and `README.md` into.
        #[arg(long, value_name = "DIR", default_value = "hurl")]
        out: PathBuf,
    },
    /// Fail if a committed Hurl suite differs from what generate would write.
    Check {
        /// Path to an OpenAPI 3.0 or 3.1 document (YAML or JSON).
        spec: PathBuf,
        /// Directory that holds the committed Hurl suite.
        #[arg(long, value_name = "DIR", default_value = "hurl")]
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(u8::try_from(err.exit_code()).unwrap_or(2))
        }
    }
}

fn run() -> spechurl::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Generate { spec, out } => {
            let suite = generate_suite(&spec)?;
            let count = suite.files.len();
            write_suite(&suite, &out)?;
            println!("wrote {count} files to {}", out.display());
            Ok(())
        }
        Commands::Check { spec, dir } => {
            let count = check_suite(&spec, &dir)?;
            println!(
                "spechurl check: OK ({count} files match {})",
                spec.display()
            );
            Ok(())
        }
    }
}
