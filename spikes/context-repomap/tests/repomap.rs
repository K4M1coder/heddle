//! Spike 3 exit criteria: repo-map + hybrid retrieval beats naive full-context.
//! Ground truth: real tree-sitter parsing + measured token ratio + middle-position.

use context_repomap::{approx_tokens, full_context, selected_context, RepoMap, SrcFile};

/// Build a sample repo where the answer to the query lives in ONE file, and
/// that file sits in the MIDDLE of the file list (middle-position case).
fn sample_repo() -> Vec<SrcFile> {
    let filler = |n: usize| SrcFile {
        path: format!("src/filler_{n}.rs"),
        code: format!(
            "// unrelated module {n}\npub struct Noise{n} {{ pub a: u32, pub b: u32 }}\n\
             pub fn helper_{n}(x: u32) -> u32 {{ x + {n} }}\n\
             pub fn more_{n}(y: u32) -> u32 {{ y * {n} }}\n"
        ),
    };
    vec![
        filler(0),
        filler(1),
        // ---- target file in the MIDDLE ----
        SrcFile {
            path: "src/payments.rs".into(),
            code: "// billing logic\n\
                   pub struct Invoice { pub total_cents: u64 }\n\
                   pub fn charge_invoice(inv: &Invoice) -> bool { inv.total_cents > 0 }\n\
                   pub trait Ledger { fn record(&self, cents: u64); }\n".into(),
        },
        filler(2),
        filler(3),
    ]
}

/// CRITERION A — tree-sitter extraction finds the right symbols in the right files.
#[test]
fn extracts_symbols_across_repo() {
    let files = sample_repo();
    let map = RepoMap::build(&files);

    // The target symbols are found and attributed to payments.rs.
    assert_eq!(map.files_for("charge_invoice"), vec!["src/payments.rs"]);
    assert_eq!(map.files_for("Invoice"), vec!["src/payments.rs"]);
    assert_eq!(map.files_for("Ledger"), vec!["src/payments.rs"]);
    // Sanity: filler symbols exist too (real parse, not a stub).
    assert!(map.symbols.iter().any(|s| s.name == "helper_0" && s.kind == "fn"));
    assert!(map.symbols.iter().any(|s| s.name == "Noise3" && s.kind == "struct"));
}

/// CRITERION B — selecting by query yields the relevant file at a FRACTION of
/// full-context tokens, and the target sits mid-list (middle-position).
#[test]
fn selected_context_is_smaller_and_sufficient() {
    let files = sample_repo();
    let map = RepoMap::build(&files);

    // Middle-position guard: target is neither first nor last.
    let idx = files.iter().position(|f| f.path == "src/payments.rs").unwrap();
    assert!(idx > 0 && idx < files.len() - 1, "target must be mid-list, got idx {idx}");

    let selected = selected_context(&files, &map, "charge_invoice");
    let full = full_context(&files);

    // Sufficiency: the selected context still contains the answer.
    assert!(selected.contains("charge_invoice"));
    assert!(selected.contains("total_cents"));

    // Economy: selected is a real fraction of full context.
    let (sel, ful) = (approx_tokens(&selected), approx_tokens(&full));
    assert!(sel < ful, "selected ({sel}) must be smaller than full ({ful})");
    let ratio = sel as f64 / ful as f64;
    assert!(ratio < 0.75, "selected should be well under full; ratio was {ratio:.2}");
    // The repo-map alone (no file bodies) is tiny — the index that scales.
    assert!(approx_tokens(&map.render()) < ful / 2, "repo-map index is compact");
}

/// CRITERION C — a query for a symbol that does not exist selects no file body
/// (no false inclusion), only the compact map remains.
#[test]
fn unknown_query_selects_no_file_body() {
    let files = sample_repo();
    let map = RepoMap::build(&files);
    let selected = selected_context(&files, &map, "nonexistent_symbol");
    assert!(selected.contains("# repo-map"));
    assert!(!selected.contains("# file:"), "no file body pulled in for an unknown symbol");
}
