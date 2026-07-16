//! Spike 4 exit criteria — MCP tool governance through a real rmcp server.
//! Ground truth: observable assertions over a live in-process client↔server pair.

use mcp_gateway::{DownstreamServer, Gateway, GatewayConfig, GatewayEvent};
use rmcp::ServiceExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Spin up a real rmcp downstream server over an in-process duplex pipe and
/// return a governed Gateway plus the shared invocation counter.
async fn setup(cfg: GatewayConfig, run_id: &str) -> (Gateway, Arc<AtomicUsize>) {
    let invocations = Arc::new(AtomicUsize::new(0));
    let (server_t, client_t) = tokio::io::duplex(8192);

    let server = DownstreamServer::new(invocations.clone());
    tokio::spawn(async move {
        if let Ok(s) = server.serve(server_t).await {
            let _ = s.waiting().await;
        }
    });

    let client = ().serve(client_t).await.expect("client connects to downstream MCP server");
    (Gateway::new(client, cfg, run_id), invocations)
}

fn cfg(approved: &[&str]) -> GatewayConfig {
    GatewayConfig {
        mutating: vec!["fs_write".into()],
        approved: approved.iter().map(|s| s.to_string()).collect(),
        secrets: vec!["sk-SECRET-abc123".into()],
    }
}

/// CRITERION 1 — a tool call is BLOCKED by policy (no approval → not executed).
#[tokio::test]
async fn c1_blocked_by_policy() {
    let (mut gw, invocations) = setup(cfg(&[]), "run-g1").await;
    let res = gw.call("fs_write").await;
    assert!(res.is_err(), "mutating tool must be denied without approval");
    assert_eq!(invocations.load(Ordering::SeqCst), 0, "downstream tool NEVER executed");
    assert!(matches!(gw.ledger().last().unwrap(), GatewayEvent::Denied { tool, .. } if tool == "fs_write"));
}

/// CRITERION 2 — the same tool is ALLOWED after approval.
#[tokio::test]
async fn c2_allowed_after_approval() {
    let (mut gw, invocations) = setup(cfg(&["fs_write"]), "run-g2").await;
    let res = gw.call("fs_write").await;
    assert!(res.is_ok(), "approved tool runs");
    assert_eq!(invocations.load(Ordering::SeqCst), 1, "downstream tool executed exactly once");
    assert!(matches!(gw.ledger().last().unwrap(), GatewayEvent::Executed { tool, .. } if tool == "fs_write"));
}

/// CRITERION 3 — the captured record has the secret REDACTED (never stored raw).
#[tokio::test]
async fn c3_capture_redacts_secret() {
    let (mut gw, _inv) = setup(cfg(&[]), "run-g3").await;
    let res = gw.call("read_secret").await.expect("read_secret allowed");

    // The live result still contains the secret (it reached the trusted caller)...
    let live = serde_json::to_string(&res.content).unwrap();
    assert!(live.contains("sk-SECRET-abc123"), "sanity: downstream really returns the secret");

    // ...but the PERSISTED record must not. Scan the whole ledger for leakage.
    let ledger_dump = format!("{:?}", gw.ledger());
    assert!(!ledger_dump.contains("sk-SECRET-abc123"), "secret must never be persisted");
    assert!(ledger_dump.contains("***"), "secret replaced by redaction marker");
}

/// CRITERION 4 — replay reconstructs outputs from the record, no downstream call.
#[tokio::test]
async fn c4_replay_from_record() {
    let (mut gw, invocations) = setup(cfg(&[]), "run-g4").await;
    gw.call("read_secret").await.unwrap();
    let calls_after_live = invocations.load(Ordering::SeqCst);
    assert_eq!(calls_after_live, 1);

    // Replay must NOT touch the downstream server, and must return the redacted record.
    let replayed = gw.replay();
    assert_eq!(invocations.load(Ordering::SeqCst), calls_after_live, "replay makes no new downstream calls");
    assert_eq!(replayed.len(), 1);
    assert!(replayed[0].contains("***") && !replayed[0].contains("sk-SECRET-abc123"));
}
