//! `skein acp-agent` — the ACP facade slice 008 built, as a running process.
//!
//! Wiring only, like every other subcommand: the protocol and its executor are
//! `skein-acp`'s, the model client is `skein-gateway`'s, the loop and the budget
//! are `skein-core`'s, and every session's chain is `skein-silo`'s.
//!
//! **stdout is the protocol.** A single byte written there that is not an ACP
//! frame corrupts the JSON-RPC stream, so this command prints nothing to it. It
//! writes one line to stderr at startup and nothing per turn — an ACP client is
//! not obliged to drain the child's stderr, and a full pipe would block the
//! agent.

use crate::wiring::{ModelArgs, NoGroundTruth, RedactArgs, RunArgs, ToolArgs};
use crate::SiloArgs;
use skein_acp::{SessionParts, SkeinAgent};
use skein_core::Result;
use skein_silo::Silo;

pub fn serve(
    silo: &SiloArgs,
    model: ModelArgs,
    redact: &RedactArgs,
    tools: ToolArgs,
    run: &RunArgs,
) -> Result<()> {
    // The Principle II guard first, so an off-machine `--base-url` opens no
    // chain and reaches no handshake — the same ordering `chat` documents.
    let endpoint = model.endpoint()?;
    // Resolved once for the process, then cloned per session below: an
    // unresolvable reference is an exit code before a single session exists.
    let redactor = redact.redactor()?;
    // Proved here rather than per session: each session builds its own
    // connector inside the factory below, long after serving began, so a
    // mistyped `--fs-root` would otherwise surface as a JSON-RPC error inside
    // an editor after a successful handshake instead of an exit code.
    tools.verify_root()?;
    // Alongside `verify_root` and for its reason: `--allow-run` on a platform
    // with no launcher must be an exit code and a message here, not a JSON-RPC
    // error an operator only meets inside an editor after a handshake that
    // already succeeded.
    let run = run.resolve()?;
    let root = silo.root()?;
    let id = silo.silo.clone();

    // Opened and dropped before serving: a bad --root or --silo is an exit code
    // and a message, not a JSON-RPC error the operator only sees inside an
    // editor after a successful handshake.
    Silo::open(&root, &id)?.ledger()?;

    eprintln!("serving acp on stdio: silo {id} at {}", endpoint.base_url());

    let budget = model.budget();
    // `model` moves into the factory: the closure outlives this frame, and one
    // client is built per session rather than shared across them.
    SkeinAgent::new(move || {
        Ok(SessionParts {
            client: model.client(endpoint.clone()),
            probe: NoGroundTruth,
            // One embedded server per session, matching the one client per
            // session above. Built here, under `futures::executor::block_on`
            // rather than a tokio runtime, which is what makes it legal at all.
            transport: tools.transport(run)?,
            // Without `--fs-root` this is an empty allowlist and nothing is
            // advertised. With one, `fs_write` is allowed **and** approved —
            // not a weakening but the only way to reach a human, because
            // `call_captured` consults the policy before the transport, so a
            // mutating tool the policy stops never becomes a permission request
            // for `AcpPermissionTransport` to ask.
            policy: tools.agent_policy(run),
            redactor: redactor.clone(),
            budget: budget.clone(),
            // One chain per session, opened here rather than shared, because
            // `SessionParts.ledger` is a `Ledger` by value. This is the whole
            // reason the factory is fallible.
            ledger: Silo::open(&root, &id)?.ledger()?,
        })
    })
    .serve_stdio()
}
