//! Spike 2 exit criteria: one real Archon-style workflow parsed → executed as a
//! stub graph → round-tripped to YAML with no semantic loss; gaps surfaced.

use workflow_archon::{execute_stub, from_archon_yaml, to_archon_yaml, NodeKind};

/// A representative Archon-style workflow exercising ALL seven node kinds.
const ARCHON_YAML: &str = r#"
name: build-and-ship
steps:
  - id: plan
    type: ai
    prompt: "draft a plan"
  - id: gate
    type: cond
    expr: "plan.ok"
    then:
      - id: code
        type: deterministic
        tool: "editor"
        args: "apply diff"
      - id: fanout
        type: parallel
        branches:
          - - id: unit
              type: tool
              tool: "cargo"
              args: "test"
          - - id: lint
              type: tool
              tool: "clippy"
              args: "-D warnings"
      - id: retry
        type: loop
        until: "tests.green"
        body:
          - id: fix
            type: agent
            prompt: "fix failing tests"
    else:
      - id: replan
        type: subagent
        workflow: "replan.yaml"
  - id: signoff
    type: human
    prompt: "approve release?"
"#;

/// PARSE + all-kinds coverage + STUB EXECUTION.
#[test]
fn parses_all_kinds_and_executes() {
    let (wf, gaps) = from_archon_yaml(ARCHON_YAML).expect("parses");
    assert!(gaps.is_empty(), "no gaps expected for known kinds, got {gaps:?}");
    assert_eq!(wf.name, "build-and-ship");

    // Every one of the 7 canonical kinds is present.
    fn collect_kinds(nodes: &[workflow_archon::Node], acc: &mut Vec<&'static str>) {
        for n in nodes {
            let label = match &n.kind {
                NodeKind::Agent { .. } => "agent",
                NodeKind::Tool { .. } => "tool",
                NodeKind::Subagent { .. } => "subagent",
                NodeKind::Approval { .. } => "approval",
                NodeKind::Cond { .. } => "cond",
                NodeKind::Parallel { .. } => "parallel",
                NodeKind::Loop { .. } => "loop",
            };
            acc.push(label);
            match &n.kind {
                NodeKind::Cond { then, otherwise, .. } => { collect_kinds(then, acc); collect_kinds(otherwise, acc); }
                NodeKind::Parallel { branches } => { for b in branches { collect_kinds(b, acc); } }
                NodeKind::Loop { body, .. } => collect_kinds(body, acc),
                _ => {}
            }
        }
    }
    let mut kinds = Vec::new();
    collect_kinds(&wf.nodes, &mut kinds);
    for k in ["agent", "tool", "subagent", "approval", "cond", "parallel", "loop"] {
        assert!(kinds.contains(&k), "kind {k} must be representable, got {kinds:?}");
    }

    // Stub execution reaches nested nodes (cond arms, parallel branches, loop body).
    let order = execute_stub(&wf);
    for id in ["plan", "gate", "code", "fanout", "unit", "lint", "retry", "fix", "replan", "signoff"] {
        assert!(order.contains(&id.to_string()), "stub exec must reach {id}, got {order:?}");
    }
}

/// SEMANTIC ROUND-TRIP: canonical → Archon YAML → canonical is identity.
#[test]
fn round_trip_is_lossless() {
    let (wf1, _) = from_archon_yaml(ARCHON_YAML).unwrap();
    let yaml2 = to_archon_yaml(&wf1);
    let (wf2, gaps) = from_archon_yaml(&yaml2).unwrap();
    assert!(gaps.is_empty());
    assert_eq!(wf1, wf2, "canonical graph survives a full YAML round-trip unchanged");
}

/// GAPS ARE SURFACED, not swallowed or panicked (FAIL-with-evidence discipline).
#[test]
fn unknown_step_type_is_recorded_as_gap() {
    let yaml = r#"
name: has-gap
steps:
  - id: weird
    type: quantum_step
  - id: ok
    type: agent
    prompt: "hi"
"#;
    let (wf, gaps) = from_archon_yaml(yaml).unwrap();
    assert_eq!(gaps.len(), 1, "the unmappable step is reported");
    assert!(gaps[0].contains("quantum_step"));
    assert_eq!(wf.nodes.len(), 1, "mappable steps still translate; gap is skipped not fatal");
}
