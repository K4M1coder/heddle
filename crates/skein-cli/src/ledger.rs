//! `skein ledger log|show|verify` — a rendering of `Ledger`'s read model.
//!
//! design §4.11 also names `replay`, `revert` and `branch`. `Ledger` has none
//! of them, and synthesising them here would be inventing core capability at the
//! outermost layer, which Constitution I forbids. They arrive when the core has
//! them.

use crate::SiloArgs;
use skein_core::{Ledger, Result, SkeinError, Step, StepKind};
use skein_silo::Silo;

/// Four tab-separated columns, unconditionally — the set does not change with
/// `--run`, so a script's field offsets never shift.
pub fn log(args: &SiloArgs, run: Option<&str>) -> Result<()> {
    let ledger = open_ledger(args)?;
    for run_id in runs(&ledger, run) {
        for step in ledger.log(run_id) {
            println!(
                "{}\t{}\t{}\t{}",
                step.run_id,
                step.seq,
                kind_name(&step.kind)?,
                step.id
            );
        }
    }
    Ok(())
}

pub fn show(args: &SiloArgs, step_id: &str) -> Result<()> {
    let ledger = open_ledger(args)?;
    let step: &Step = ledger.show(step_id)?;
    println!("id\t{}", step.id);
    println!("parent\t{}", step.parent.as_deref().unwrap_or("-"));
    println!("run\t{}", step.run_id);
    println!("seq\t{}", step.seq);
    println!("kind\t{}", kind_name(&step.kind)?);
    println!("payload");
    println!("{}", step.payload);
    Ok(())
}

/// Reports each run that verified, and stops at the first break: a chain is
/// only worth reading up to the point where it stopped being trustworthy.
pub fn verify(args: &SiloArgs, run: Option<&str>) -> Result<()> {
    let ledger = open_ledger(args)?;
    for run_id in runs(&ledger, run) {
        ledger.verify_chain(run_id)?;
        println!("{run_id}\tok\t{} steps", ledger.log(run_id).len());
    }
    Ok(())
}

fn runs<'a>(ledger: &'a Ledger, run: Option<&'a str>) -> Vec<&'a str> {
    match run {
        Some(run_id) => vec![run_id],
        None => ledger.runs(),
    }
}

/// The step kind's **serde** name — the same string the hash function is fed.
/// Matching on the enum here would be a second name mapping that could drift
/// from the hashed bytes, which is the rule `skein-silo`'s store states for
/// itself.
fn kind_name(kind: &StepKind) -> Result<String> {
    match serde_json::to_value(kind)? {
        serde_json::Value::String(name) => Ok(name),
        // Unreachable while `StepKind`'s variants are all unit variants, which
        // the type system cannot state; refusing beats printing a blank column.
        other => Err(SkeinError::Storage(format!(
            "step kind {other} did not serialise to a name"
        ))),
    }
}

/// Resolves the silo and opens its chain.
///
/// `Silo::open` both validates the id — the containment property, which must
/// not be re-implemented here — and `create_dir_all`s. So an unknown silo would
/// otherwise yield an empty log and exit 0, which is a silently wrong answer to
/// "what did this agent do". Requiring the ledger file to already exist turns
/// that into a loud failure; the empty directory left behind is an accepted
/// wart, recorded in `specs/011-skein-cli/spec.md`.
fn open_ledger(args: &SiloArgs) -> Result<Ledger> {
    let silo = Silo::open(args.root()?, &args.silo)?;
    let path = silo.ledger_path();
    if !path.exists() {
        return Err(SkeinError::NotFound(format!(
            "silo {:?} has no ledger at {}",
            args.silo,
            path.display()
        )));
    }
    silo.ledger()
}
