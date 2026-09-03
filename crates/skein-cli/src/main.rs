//! `skein` — the reference CLI client (design §4.1, Constitution I).
//!
//! The core is a library and every capability it has is reachable through a
//! public API; this binary is that API's complete, authoritative client and the
//! surface the end-to-end tests drive. It therefore holds **no** capability of
//! its own: each subcommand is a call onto `skein-core`/`skein-silo` plus a
//! rendering of the result.
//!
//! v0 has no `chat` and no `acp-agent` because the workspace has no real
//! `ModelClient` to put behind them; see `specs/011-skein-cli/spec.md`.

mod ledger;
mod secret;

use clap::{Args, Parser, Subcommand};
use skein_core::Result;
use std::path::PathBuf;
use std::process::ExitCode;

/// `bin_name` is set explicitly so usage text reads `skein …` on every OS —
/// without it clap derives `skein.exe` on Windows from the executable name.
#[derive(Parser)]
#[command(
    name = "skein",
    bin_name = "skein",
    version,
    about = "Inspect a Skein silo's ledger and provision its secrets"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read a silo's append-only, hash-chained journal.
    Ledger {
        #[command(subcommand)]
        command: LedgerCommand,
    },
    /// Put a secret in the platform credential store, or remove one.
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },
}

/// Which silo a ledger command reads. There is no config file in v0, so the
/// root is named on every invocation or in the environment.
#[derive(Args)]
pub struct SiloArgs {
    /// Directory holding the silos. Defaults to $SKEIN_ROOT.
    #[arg(long, value_name = "PATH")]
    root: Option<PathBuf>,
    /// Silo id: one path component of [A-Za-z0-9._-].
    #[arg(long, value_name = "ID")]
    silo: String,
}

#[derive(Subcommand)]
enum LedgerCommand {
    /// One line per step: run_id, seq, kind and id, tab-separated.
    Log {
        #[command(flatten)]
        silo: SiloArgs,
        /// Only this run. Omitted, every run in the silo is listed.
        #[arg(long, value_name = "RUN_ID")]
        run: Option<String>,
    },
    /// Print one step's header and its payload verbatim.
    Show {
        #[command(flatten)]
        silo: SiloArgs,
        /// The step id, as printed by `skein ledger log`.
        #[arg(value_name = "STEP_ID")]
        step_id: String,
    },
    /// Recompute the hash chain and report the first break, if any.
    Verify {
        #[command(flatten)]
        silo: SiloArgs,
        /// Only this run. Omitted, every run in the silo is verified.
        #[arg(long, value_name = "RUN_ID")]
        run: Option<String>,
    },
}

#[derive(Subcommand)]
enum SecretCommand {
    /// Store a secret. The value is read from stdin, never from a flag.
    Set {
        /// keychain://<service>/<account>
        #[arg(value_name = "REFERENCE")]
        reference: String,
    },
    /// Remove a secret from the platform credential store.
    Delete {
        /// keychain://<service>/<account>
        #[arg(value_name = "REFERENCE")]
        reference: String,
    },
}

/// One boundary turns an error into a message and an exit code, so no command
/// has to remember to — and so a command that fails has written nothing to
/// stdout by the time it does.
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Ledger { command } => match command {
            LedgerCommand::Log { silo, run } => ledger::log(&silo, run.as_deref()),
            LedgerCommand::Show { silo, step_id } => ledger::show(&silo, &step_id),
            LedgerCommand::Verify { silo, run } => ledger::verify(&silo, run.as_deref()),
        },
        Command::Secret { command } => match command {
            SecretCommand::Set { reference } => secret::set(&reference),
            SecretCommand::Delete { reference } => secret::delete(&reference),
        },
    }
}
