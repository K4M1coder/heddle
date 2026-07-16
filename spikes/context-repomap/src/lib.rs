//! SPIKE 3 — context quality (quarantined per ADR-0004 D2, throwaway).
//! Proves: extract symbols via tree-sitter → build a repo-map → select the
//! relevant file(s) for a query at a fraction of full-context tokens, including
//! a middle-position case. Ground truth = tests/repomap.rs.

use tree_sitter::{Parser, Query, QueryCursor};
use streaming_iterator::StreamingIterator;

/// A source file in the (in-memory) sample repo.
pub struct SrcFile {
    pub path: String,
    pub code: String,
}

/// One extracted symbol (fn / struct / enum / trait) with the file it lives in.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub path: String,
}

/// Cheap token proxy: whitespace-split word count (deterministic, good enough
/// to demonstrate the ratio; not a real BPE tokenizer).
pub fn approx_tokens(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Extract top-level symbols from one Rust file using tree-sitter.
pub fn extract_symbols(file: &SrcFile) -> Vec<Symbol> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).expect("load rust grammar");
    let tree = parser.parse(&file.code, None).expect("parse");

    // Capture the name identifier of each definition kind.
    let query_src = r#"
        (function_item name: (identifier) @fn)
        (struct_item name: (type_identifier) @struct)
        (enum_item name: (type_identifier) @enum)
        (trait_item name: (type_identifier) @trait)
    "#;
    let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), query_src).expect("valid query");
    let names: Vec<&str> = query.capture_names().to_vec();

    let mut cursor = QueryCursor::new();
    let mut out = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), file.code.as_bytes());
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let kind = names[cap.index as usize].to_string();
            let name = cap.node.utf8_text(file.code.as_bytes()).unwrap_or("").to_string();
            out.push(Symbol { name, kind, path: file.path.clone() });
        }
    }
    out
}

/// A repo-map: every symbol across the repo, plus its file. This is the compact
/// index the model sees instead of every full file.
pub struct RepoMap {
    pub symbols: Vec<Symbol>,
}

impl RepoMap {
    pub fn build(files: &[SrcFile]) -> Self {
        RepoMap { symbols: files.iter().flat_map(extract_symbols).collect() }
    }

    /// One compact line per symbol — this is what goes into the prompt.
    pub fn render(&self) -> String {
        self.symbols.iter().map(|s| format!("{}\t{}\t{}", s.path, s.kind, s.name)).collect::<Vec<_>>().join("\n")
    }

    /// Resolve a query (a symbol name) to the file(s) that define it.
    pub fn files_for(&self, query: &str) -> Vec<String> {
        let mut v: Vec<String> = self.symbols.iter().filter(|s| s.name == query).map(|s| s.path.clone()).collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Selected-context strategy: repo-map + only the files that define the query symbol.
pub fn selected_context(files: &[SrcFile], map: &RepoMap, query: &str) -> String {
    let mut ctx = format!("# repo-map\n{}\n", map.render());
    for path in map.files_for(query) {
        if let Some(f) = files.iter().find(|f| f.path == path) {
            ctx.push_str(&format!("\n# file: {}\n{}\n", f.path, f.code));
        }
    }
    ctx
}

/// Baseline strategy: dump every file in full.
pub fn full_context(files: &[SrcFile]) -> String {
    files.iter().map(|f| format!("# file: {}\n{}\n", f.path, f.code)).collect::<Vec<_>>().join("\n")
}
