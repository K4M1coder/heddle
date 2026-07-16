# Spike 3 Evidence — Context quality (`spikes/context-repomap/`)

**Date:** 2026-07-16 · **Status:** COMPLETE (representative-scale) · **Method:** spike-protocol.md §Spike 3; ground truth = 3 passing tests using real tree-sitter parsing.

## Question
Does a repo-map (tree-sitter symbols) + query selection beat naive full-file context — smaller yet sufficient — including a middle-position target?

## Result — PASS (at representative scale)

| Exit criterion | Proof (tests/repomap.rs) |
|---|---|
| Real symbol extraction | `extracts_symbols_across_repo`: `tree-sitter` + `tree-sitter-rust` extract fn/struct/enum/trait and attribute them to files; `charge_invoice`/`Invoice`/`Ledger` → `src/payments.rs`; filler symbols also parsed (genuine parse, not a stub) |
| Smaller yet sufficient | `selected_context_is_smaller_and_sufficient`: selected context still contains the answer (`charge_invoice`, `total_cents`) AND is a real fraction of full context (`ratio < 0.75`; repo-map index alone `< full/2`) |
| Middle position | same test guards the target is neither first nor last in the file list |
| No false inclusion | `unknown_query_selects_no_file_body`: an unknown symbol pulls in the compact map only, zero file bodies |

`cargo test` → **3 passed / 0 failed**.

## Findings
- The **repo-map is the part that scales**: one compact `path⇥kind⇥name` line per symbol, no file bodies — the index the model sees instead of the whole tree, exactly the design's "smallest sufficient context".
- Selection is **ACL-ready by construction**: `files_for(query)` returns paths, so the same step can be filtered by silo/scope permissions before any body is included (ties to §7.10 RBAC / context manifest).

## Caveats (honest scope)
- **Token proxy** is whitespace word-count, not a real BPE tokenizer — fine to demonstrate the ratio, not to quote absolute token counts.
- **Retrieval is exact-name lookup**, not the full hybrid (BM25 + embeddings + dependency-graph ranking) the design calls for; this proves the *repo-map + selection* leg, not ranking quality on ambiguous queries.
- **Scale is a 5-file in-memory repo**, not a large real repository; the *lost-in-the-middle* degradation of long-context models (RULER/LongCodeBench) is argued from literature, not re-measured here. A large-repo benchmark against a real model remains follow-up.

## Consequence for ADR-0003
Spike 3 of 5 complete at representative scale: repo-map + selection is viable and the compact-index approach holds. Full hybrid-retrieval quality benchmarking is deferred (bucket C). Spikes 1, 2, 3, 4 done; only Spike 5 (tri-OS offline install) remains and needs CI/macOS+Linux.
