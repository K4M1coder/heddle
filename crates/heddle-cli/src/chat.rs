//! `heddle chat` — one prompt, one governed run against a local model provider.
//!
//! The first command that runs the loop rather than reading its record. It holds
//! no capability of its own: the client is `heddle-gateway`'s, the loop and the
//! budget are `heddle-core`'s, and the chain is `heddle-silo`'s. What lives here
//! is the wiring and the output contract.
//!
//! **stdout carries the assistant's answer and nothing else.** The run id goes
//! to stderr, so stdout stays the scriptable contract slice 011 established, and
//! a run the engine stopped prints nothing at all rather than an empty answer
//! that looks like an answer.

use crate::wiring::NoGroundTruth;
use crate::{ChatArgs, SiloArgs};
use heddle_connectors::RunAccess;
use heddle_core::{Exit, HeddleError, LoopController, Message, NativeLoop, Result, ToolGateway};
use heddle_silo::Silo;
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn chat(silo: &SiloArgs, args: &ChatArgs) -> Result<()> {
    // Before the silo is touched, so a refused endpoint opens no chain: an
    // endpoint that cannot be built is an endpoint no socket was opened to, and
    // a silo with a one-step run in it would be a misleading record of an
    // attempt that never left the process. That reasoning is why `--provider`
    // resolves here too — an unreadable provider table, an unknown provider
    // name and a route refused for egress are all failures of the same kind,
    // and they must be exit codes at the same point in the sequence.
    //
    // `--provider` wins when it is given, and `--base-url`/`--model` are the
    // path when it is not. There is no merge: a named provider carries its own
    // address and its own model, so taking half of each would produce a
    // configuration no operator wrote down. `--timeout-secs` is threaded
    // through both, because it is a budget for the request rather than a
    // property of the provider.
    let model_client = match args
        .provider
        .client(Duration::from_secs(args.model.timeout_secs))?
    {
        Some(client) => client,
        None => args.model.client(args.model.endpoint()?),
    };
    // Before the silo for the same reason, and after the endpoint so the three
    // refusals keep the order both commands document.
    let redactor = args.redact.redactor()?;
    // Third and last of the pre-flight refusals, in the order this command
    // documents and `acp` mirrors: a `--fs-root` that does not exist is an exit
    // code before the silo is touched, so no chain holds a one-step run for an
    // attempt that never left the process.
    // `RunAccess::Denied`, always: this command has no `--allow-run` to
    // resolve, so no sandbox is built and no directory's ACL is touched.
    // The flag is minted here and dropped here: `chat` is non-interactive and
    // has no cancel channel, so "nothing can stop this run" is stated in the
    // wiring rather than asserted in a comment. Giving it one is a CLI slice
    // with its own decisions.
    let transport = args
        .tools
        .transport(RunAccess::Denied, Arc::new(AtomicBool::new(false)))?;
    let prompt = prompt(args.prompt.as_deref())?;

    let run_id = match &args.run_id {
        Some(id) => id.clone(),
        None => minted_run_id(),
    };
    let mut ledger = Silo::open(silo.root()?, &silo.silo)?.ledger()?;
    let mut controller = LoopController::new(args.model.budget());
    let mut loops = NativeLoop::new(
        model_client,
        NoGroundTruth,
        ToolGateway::new(
            transport,
            // Without `--fs-root` this is still an empty allowlist, so no tool
            // name reaches the transport and nothing is advertised. With one,
            // it is the two read-only fs tools and **not** `fs_write`: this
            // command is non-interactive, and Constitution VI does not let a
            // destructive tool run with nobody to confirm it. `wiring`'s
            // `chat_policy` carries the argument.
            args.tools.chat_policy(),
            // The same secret set on both sides of the run: the gateway and the
            // loop write into one chain.
            redactor.clone(),
        ),
        redactor,
    );

    // Before the answer, so a run whose id the operator needs is named even
    // when the run then fails.
    eprintln!("run {run_id}");
    let run = loops.run(&run_id, prompt, &mut ledger, &mut controller)?;

    match (run.exit, run.final_message) {
        (Exit::FinalOutput, Some(message)) => {
            println!("{}", message.text());
            Ok(())
        }
        // `LoopRun.final_message` is `None` for every non-`FinalOutput` exit, so
        // there is nothing to print — and printing nothing with exit 0 would be
        // an empty answer that looks like an answer.
        (exit, _) => Err(HeddleError::Unfinished {
            run_id,
            exit: format!("{exit:?}"),
        }),
    }
}

/// `--prompt`, else stdin to EOF. One prompt, one run: there is no REPL, so
/// stdin is read once and never again.
fn prompt(flag: Option<&str>) -> Result<Message> {
    let text = match flag {
        Some(text) => text.to_string(),
        None => {
            let mut piped = String::new();
            std::io::stdin().read_to_string(&mut piped).map_err(|e| {
                HeddleError::Model(format!("could not read the prompt from stdin: {e}"))
            })?;
            piped
        }
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(HeddleError::Model(
            "the prompt is empty; pass --prompt <TEXT> or pipe one on stdin".into(),
        ));
    }
    Ok(Message::user_text(text))
}

/// Unique per invocation, because reusing an id would put two `Exit` steps in
/// one chain — the thing `heddle-acp`'s `{session_id}#{n}` scheme already avoids.
fn minted_run_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    format!("chat-{millis}-{}", std::process::id())
}
