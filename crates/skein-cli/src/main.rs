//! `skein` — the reference CLI client (design §4.1, Constitution I).
//!
//! The core is a library and every capability it has is reachable through a
//! public API; this binary is that API's complete, authoritative client and the
//! surface the end-to-end tests drive. It therefore holds **no** capability of
//! its own: each subcommand is a call onto `skein-core`/`skein-silo` plus a
//! rendering of the result.
//!
//! Two commands *run* the loop rather than reading its record. `skein chat`
//! takes one prompt and prints one answer; `skein acp-agent` serves slice 008's
//! ACP facade on stdin/stdout so an editor can drive the same loop. Both reach
//! the same loopback-only provider through the same `wiring::ModelArgs`, and
//! both record every run on the silo's chain.
//!
//! Both also take `--fs-root` — the one directory an agent may work in, and the
//! only way either command gains a tool. It is opt-in, and it names two
//! families: the filesystem tools always, and the git tools when that
//! directory is a git repository. The two commands allowlist **different**
//! tools behind it: `wiring::ToolArgs` holds the reason.
//!
//! `skein acp-agent` alone takes a **second** opt-in, `--allow-run`, for a
//! third family: one sandboxed process launcher, Windows-only in v0.
//! `wiring::RunArgs` holds that reason too.

mod acp;
mod chat;
mod ledger;
mod sandbox;
mod secret;
mod wiring;

use clap::{Args, Parser, Subcommand};
use skein_core::{Result, SkeinError};
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
    /// Inspect or remove the app container profiles and directory grants that
    /// `--allow-run` sessions leave on this machine.
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommand,
    },
    /// Ask a local model one question, recording the run on the silo's chain.
    Chat {
        #[command(flatten)]
        silo: SiloArgs,
        #[command(flatten)]
        chat: ChatArgs,
    },
    /// Serve the Agent Client Protocol on stdin/stdout, for an ACP-speaking
    /// editor to drive. Every session's runs land on the silo's chain.
    AcpAgent {
        #[command(flatten)]
        silo: SiloArgs,
        #[command(flatten)]
        model: wiring::ModelArgs,
        #[command(flatten)]
        redact: wiring::RedactArgs,
        #[command(flatten)]
        tools: wiring::ToolArgs,
        /// Flattened here and into **no other** subcommand: `wiring::RunArgs`
        /// holds the reason, and `skein chat` having nobody to ask for
        /// permission is most of it.
        #[command(flatten)]
        run: wiring::RunArgs,
    },
}

/// Which silo a command reads or writes. There is no config file in v0, so the
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

impl SiloArgs {
    /// `--root`, else `$SKEIN_ROOT`, else a loud refusal. v0 has no config file
    /// and no platform data directory, so guessing a root would put an agent's
    /// journal somewhere the operator did not name.
    pub fn root(&self) -> Result<PathBuf> {
        self.root.clone().map(Ok).unwrap_or_else(|| {
            std::env::var_os("SKEIN_ROOT")
                .map(Into::into)
                .ok_or_else(|| {
                    SkeinError::Storage("no silo root: pass --root or set SKEIN_ROOT".into())
                })
        })
    }
}

/// The knobs `skein chat` needs beyond the silo and the model.
#[derive(Args)]
pub struct ChatArgs {
    #[command(flatten)]
    model: wiring::ModelArgs,
    #[command(flatten)]
    redact: wiring::RedactArgs,
    #[command(flatten)]
    tools: wiring::ToolArgs,
    /// The prompt. Omitted, it is read from stdin to EOF.
    #[arg(long, value_name = "TEXT")]
    prompt: Option<String>,
    /// Run id to record under. Defaults to chat-{unix_millis}-{pid}.
    #[arg(long, value_name = "ID")]
    run_id: Option<String>,
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

/// Present on every platform, and it refuses with a reason off Windows rather
/// than being absent.
///
/// The opposite of the rule a *tool* follows, deliberately. A model calling an
/// allowlisted-but-disabled tool gets a fatal run, so a tool must not be
/// advertised where it cannot work; a subcommand is read by a human, and one
/// that is simply missing is indistinguishable from a stale binary or a typo.
#[derive(Subcommand)]
enum SandboxCommand {
    /// One tab-separated line per directory a profile was granted, with what
    /// its permissions say right now.
    List,
    /// Revoke a profile's grants and delete it. Exactly one selector, always:
    /// a bare `prune` is a usage error, never a machine-wide delete.
    Prune {
        #[command(flatten)]
        selector: PruneSelector,
    },
}

/// The selector is a clap group rather than a hand-written check, so both
/// "neither" and "both" are refused by the parser before `run` is entered.
#[derive(Args)]
#[group(required = true, multiple = false)]
struct PruneSelector {
    /// One profile, named as `skein sandbox list` prints it.
    #[arg(long, value_name = "NAME")]
    profile: Option<String>,
    /// Every profile skein made on this machine.
    #[arg(long)]
    all: bool,
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
        Command::Sandbox { command } => match command {
            SandboxCommand::List => sandbox::list(),
            SandboxCommand::Prune { selector } => {
                sandbox::prune(selector.profile.as_deref(), selector.all)
            }
        },
        Command::Chat { silo, chat } => chat::chat(&silo, &chat),
        Command::AcpAgent {
            silo,
            model,
            redact,
            tools,
            run,
        } => acp::serve(&silo, model, &redact, tools, &run),
    }
}
