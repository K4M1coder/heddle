//! SPIKE 2 — workflow reuse (quarantined per ADR-0004 D2, throwaway).
//! Proves an Archon-style YAML workflow maps losslessly onto Heddle's canonical
//! graph (nodes: agent/tool/subagent/approval/cond/parallel/loop) and round-trips.
//! Ground truth = tests/roundtrip.rs.

use serde::{Deserialize, Serialize};

// ---------- Heddle canonical graph (the owned contract) ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    Agent { prompt: String },
    Tool { tool: String, args: String },
    Subagent { workflow: String },
    Approval { prompt: String },
    Cond { expr: String, then: Vec<Node>, otherwise: Vec<Node> },
    Parallel { branches: Vec<Vec<Node>> },
    Loop { until: String, body: Vec<Node> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(flatten)]
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub nodes: Vec<Node>,
}

// ---------- Archon-style external shape (distinct field vocabulary) ----------
// Archon workflows use `steps` with a `type` discriminator and step-specific
// fields; we translate that vocabulary into the canonical graph above.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchonWorkflow {
    pub name: String,
    pub steps: Vec<ArchonStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchonStep {
    pub id: String,
    #[serde(rename = "type")]
    pub step_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub then: Vec<ArchonStep>,
    #[serde(default, rename = "else", skip_serializing_if = "Vec::is_empty")]
    pub otherwise: Vec<ArchonStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<Vec<ArchonStep>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<ArchonStep>,
}

/// Translation gaps (a step type we cannot represent) are collected, not panicked.
pub type Gaps = Vec<String>;

fn steps_to_nodes(steps: &[ArchonStep], gaps: &mut Gaps) -> Vec<Node> {
    steps.iter().filter_map(|s| step_to_node(s, gaps)).collect()
}

fn step_to_node(s: &ArchonStep, gaps: &mut Gaps) -> Option<Node> {
    let kind = match s.step_type.as_str() {
        "agent" | "ai" => NodeKind::Agent { prompt: s.prompt.clone().unwrap_or_default() },
        "tool" | "deterministic" => NodeKind::Tool {
            tool: s.tool.clone().unwrap_or_default(),
            args: s.args.clone().unwrap_or_default(),
        },
        "subagent" | "sub_workflow" => NodeKind::Subagent { workflow: s.workflow.clone().unwrap_or_default() },
        "approval" | "human" => NodeKind::Approval { prompt: s.prompt.clone().unwrap_or_default() },
        "cond" | "conditional" => NodeKind::Cond {
            expr: s.expr.clone().unwrap_or_default(),
            then: steps_to_nodes(&s.then, gaps),
            otherwise: steps_to_nodes(&s.otherwise, gaps),
        },
        "parallel" => NodeKind::Parallel {
            branches: s.branches.iter().map(|b| steps_to_nodes(b, gaps)).collect(),
        },
        "loop" => NodeKind::Loop {
            until: s.until.clone().unwrap_or_default(),
            body: steps_to_nodes(&s.body, gaps),
        },
        other => {
            gaps.push(format!("unmappable step '{}' (type '{}')", s.id, other));
            return None;
        }
    };
    Some(Node { id: s.id.clone(), kind })
}

fn nodes_to_steps(nodes: &[Node]) -> Vec<ArchonStep> {
    nodes.iter().map(node_to_step).collect()
}

fn base_step(id: &str, ty: &str) -> ArchonStep {
    ArchonStep {
        id: id.to_string(), step_type: ty.to_string(),
        prompt: None, tool: None, args: None, workflow: None, expr: None, until: None,
        then: vec![], otherwise: vec![], branches: vec![], body: vec![],
    }
}

fn node_to_step(n: &Node) -> ArchonStep {
    match &n.kind {
        NodeKind::Agent { prompt } => { let mut s = base_step(&n.id, "agent"); s.prompt = Some(prompt.clone()); s }
        NodeKind::Tool { tool, args } => { let mut s = base_step(&n.id, "tool"); s.tool = Some(tool.clone()); s.args = Some(args.clone()); s }
        NodeKind::Subagent { workflow } => { let mut s = base_step(&n.id, "subagent"); s.workflow = Some(workflow.clone()); s }
        NodeKind::Approval { prompt } => { let mut s = base_step(&n.id, "approval"); s.prompt = Some(prompt.clone()); s }
        NodeKind::Cond { expr, then, otherwise } => {
            let mut s = base_step(&n.id, "cond"); s.expr = Some(expr.clone());
            s.then = nodes_to_steps(then); s.otherwise = nodes_to_steps(otherwise); s
        }
        NodeKind::Parallel { branches } => {
            let mut s = base_step(&n.id, "parallel");
            s.branches = branches.iter().map(|b| nodes_to_steps(b)).collect(); s
        }
        NodeKind::Loop { until, body } => {
            let mut s = base_step(&n.id, "loop"); s.until = Some(until.clone()); s.body = nodes_to_steps(body); s
        }
    }
}

/// Archon YAML → canonical Workflow (+ gaps).
pub fn from_archon_yaml(yaml: &str) -> Result<(Workflow, Gaps), String> {
    let aw: ArchonWorkflow = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
    let mut gaps = Gaps::new();
    let nodes = steps_to_nodes(&aw.steps, &mut gaps);
    Ok((Workflow { name: aw.name, nodes }, gaps))
}

/// Canonical Workflow → Archon YAML.
pub fn to_archon_yaml(wf: &Workflow) -> String {
    let aw = ArchonWorkflow { name: wf.name.clone(), steps: nodes_to_steps(&wf.nodes) };
    serde_yaml::to_string(&aw).unwrap()
}

/// Stub executor: pre-order visit of the graph, proving every node kind is
/// reachable and traversable (branches/loop body/cond arms included).
pub fn execute_stub(wf: &Workflow) -> Vec<String> {
    fn walk(nodes: &[Node], out: &mut Vec<String>) {
        for n in nodes {
            out.push(n.id.clone());
            match &n.kind {
                NodeKind::Cond { then, otherwise, .. } => { walk(then, out); walk(otherwise, out); }
                NodeKind::Parallel { branches } => { for b in branches { walk(b, out); } }
                NodeKind::Loop { body, .. } => { walk(body, out); }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&wf.nodes, &mut out);
    out
}
