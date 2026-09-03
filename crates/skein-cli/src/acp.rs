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

use crate::wiring::{ModelArgs, NoGroundTruth, NoTools};
use crate::SiloArgs;
use skein_acp::{SessionParts, SkeinAgent};
use skein_core::{Redactor, Result, ToolPolicy};
use skein_silo::Silo;

pub fn serve(silo: &SiloArgs, model: ModelArgs) -> Result<()> {
    // The Principle II guard first, so an off-machine `--base-url` opens no
    // chain and reaches no handshake — the same ordering `chat` documents.
    let endpoint = model.endpoint()?;
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
            transport: NoTools,
            // Deny-by-default with an empty allowlist: no tool name can reach
            // the transport, because the policy refuses every one of them
            // first. It also means `AcpPermissionTransport` is unreachable here
            // until tool advertisement exists — a model that invents a tool
            // call gets a denial the client sees as a failed tool call.
            policy: ToolPolicy::new(vec![], vec![]),
            redactor: Redactor::new(vec![]),
            budget: budget.clone(),
            // One chain per session, opened here rather than shared, because
            // `SessionParts.ledger` is a `Ledger` by value. This is the whole
            // reason the factory is fallible.
            ledger: Silo::open(&root, &id)?.ledger()?,
        })
    })
    .serve_stdio()
}
