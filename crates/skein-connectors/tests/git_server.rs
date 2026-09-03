//! The two git tools, exercised as the server's own methods (spec 017, SC-005,
//! SC-006, SC-009).
//!
//! These call the `#[tool]` methods directly, which is the level that can show
//! a *tool-level* refusal as an `Err(String)` before rmcp turns it into a
//! `CallToolResult { is_error: true }` — `fs_server.rs`'s precedent. The
//! end-to-end proof that a refusal really arrives that way is
//! `governed_git_run.rs`.
//!
//! Every fixture is a real repository with real commits, built through `git2`.
//! Each one sets `HEAD` to `refs/heads/work` **before** its first commit, so
//! the exact-string assertions on the `## <branch>` header do not silently
//! depend on whether the machine running them has `init.defaultBranch`
//! configured.

use git2::{Repository, Signature};
use rmcp::handler::server::wrapper::Parameters;
use skein_connectors::{
    is_git_repository, EmbeddedServer, FsRoot, LogParams, LOG_COUNT_CAP, STATUS_ENTRY_CAP,
};
use std::path::Path;
use tempfile::TempDir;

/// The branch every fixture is on: named here rather than inherited from the
/// machine's git configuration.
const BRANCH: &str = "work";

struct Fixture {
    dir: TempDir,
    repo: Repository,
    server: EmbeddedServer,
}

/// An empty repository on [`BRANCH`], with no commit yet.
fn unborn() -> Fixture {
    let dir = TempDir::new().expect("a temp dir");
    let repo = Repository::init(dir.path()).expect("a repository is initialised");
    repo.set_head(&format!("refs/heads/{BRANCH}"))
        .expect("HEAD names the fixture's branch");
    Fixture {
        server: EmbeddedServer::new(FsRoot::new(dir.path()).expect("a canonicalizable root")),
        repo,
        dir,
    }
}

impl Fixture {
    /// Writes `contents` to `name` under the worktree.
    fn write(&self, name: &str, contents: &str) -> &Fixture {
        std::fs::write(self.dir.path().join(name), contents).expect("a file in the worktree");
        self
    }

    /// Stages `name` without committing it.
    fn stage(&self, name: &str) -> &Fixture {
        let mut index = self.repo.index().expect("the index opens");
        index.add_path(Path::new(name)).expect("the path is staged");
        index.write().expect("the index is written");
        self
    }

    /// Commits whatever is staged, with `message`.
    fn commit(&self, message: &str) -> &Fixture {
        let mut index = self.repo.index().expect("the index opens");
        let tree = self
            .repo
            .find_tree(index.write_tree().expect("the index writes a tree"))
            .expect("the tree is found");
        let who = Signature::now("Fixture Author", "fixture@example.invalid").expect("a signature");
        let parents: Vec<git2::Commit> = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .into_iter()
            .collect();
        self.repo
            .commit(
                Some("HEAD"),
                &who,
                &who,
                message,
                &tree,
                &parents.iter().collect::<Vec<_>>(),
            )
            .expect("the commit is written");
        self
    }

    fn status(&self) -> Result<String, String> {
        self.server.git_status()
    }

    fn log(&self, count: u32) -> Result<String, String> {
        self.server.git_log(Parameters(LogParams { count }))
    }
}

/// A repository on [`BRANCH`] holding one committed file.
fn committed() -> Fixture {
    let f = unborn();
    f.write("tracked.txt", "first version\n")
        .stage("tracked.txt")
        .commit("add the tracked file");
    f
}

// ---------------------------------------------------------------------------
// SC-005 — `git_status`.
// ---------------------------------------------------------------------------

#[test]
fn git_status_reports_the_branch_and_the_staged_and_worktree_changes_in_porcelain_form() {
    let f = committed();
    f.write("tracked.txt", "second version\n")
        .write("added.txt", "brand new\n")
        .stage("added.txt")
        .write("untracked.txt", "nobody knows about me\n");

    // Porcelain v1's own two-character codes rather than prose, because a model
    // has seen millions of lines of them: `A ` staged addition, ` M` worktree
    // modification, `??` untracked. Entries sorted by path, so the same
    // worktree reads the same way twice (`fs_list`'s precedent).
    assert_eq!(
        f.status()
            .expect("a contained repository reports its status"),
        "## work\nA \tadded.txt\n M\ttracked.txt\n??\tuntracked.txt"
    );
}

#[test]
fn git_status_says_plainly_when_the_working_tree_is_clean() {
    let f = committed();

    assert_eq!(
        f.status().expect("a clean repository reports its status"),
        "## work\n# working tree clean"
    );
}

#[test]
fn git_status_caps_its_entries_and_says_how_many_it_did_not_show() {
    let f = committed();
    let over = STATUS_ENTRY_CAP + 37;
    for i in 0..over {
        f.write(&format!("file-{i:04}.txt"), "untracked\n");
    }

    let status = f.status().expect("an oversized status still reports");

    // Truncated **and labelled**, where `fs_read` refuses. The asymmetry is
    // deliberate: `git_status` takes no arguments, so there is no smaller call
    // for the model to make and a refusal would leave a dirty repository
    // permanently unreadable. A silent truncation would be a wrong answer in a
    // right answer's shape; this says what it dropped.
    let lines: Vec<&str> = status.lines().collect();
    assert_eq!(lines[0], "## work");
    assert_eq!(
        lines.len(),
        STATUS_ENTRY_CAP + 2,
        "one header, {STATUS_ENTRY_CAP} entries and one notice: {status}"
    );
    assert_eq!(
        lines[lines.len() - 1],
        format!("# 37 more entries not shown, over the {STATUS_ENTRY_CAP}-entry cap")
    );
    assert_eq!(lines[1], "??\tfile-0000.txt", "sorted by path: {status}");
    assert!(
        !status.contains(&format!("file-{:04}.txt", over - 1)),
        "the last entry by sort order must be the one dropped: {status}"
    );
}

#[test]
fn git_status_names_an_unborn_branch_rather_than_failing() {
    let f = unborn();
    f.write("first.txt", "not committed yet\n");

    // `repo.head()` fails with `UnbornBranch` here, and libgit2's own message
    // is `reference 'refs/heads/work' not found` — true and useless to a model
    // that just wants to know where it is.
    assert_eq!(
        f.status()
            .expect("an unborn branch is a state, not a failure"),
        "## (unborn branch work)\n??\tfirst.txt"
    );
}

#[test]
fn git_status_names_a_detached_head() {
    let f = committed();
    let head = f
        .repo
        .head()
        .expect("a born branch")
        .peel_to_commit()
        .expect("the commit HEAD points at");
    f.repo
        .set_head_detached(head.id())
        .expect("HEAD is detached");

    let status = f
        .status()
        .expect("a detached HEAD is a state, not a failure");

    assert_eq!(
        status,
        format!(
            "## (detached HEAD at {})\n# working tree clean",
            &head.id().to_string()[..7]
        )
    );
}

// ---------------------------------------------------------------------------
// SC-006 — `git_log`.
// ---------------------------------------------------------------------------

#[test]
fn git_log_returns_the_most_recent_commits_newest_first() {
    let f = committed();
    f.write("second.txt", "two\n")
        .stage("second.txt")
        .commit("the second commit");
    f.write("third.txt", "three\n")
        .stage("third.txt")
        .commit("the third commit");

    let log = f.log(3).expect("three commits are readable");

    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 3, "{log}");
    assert_eq!(
        lines
            .iter()
            .map(|l| l.split('\t').nth(3).expect("a summary column"))
            .collect::<Vec<_>>(),
        vec![
            "the third commit",
            "the second commit",
            "add the tracked file"
        ],
        "newest first: {log}"
    );
    for line in &lines {
        let columns: Vec<&str> = line.split('\t').collect();
        assert_eq!(columns.len(), 4, "oid, date, author, summary: {line}");
        assert_eq!(columns[0].len(), 7, "a seven-hex short oid: {line}");
        assert!(
            columns[0].chars().all(|c| c.is_ascii_hexdigit()),
            "a seven-hex short oid: {line}"
        );
        // UTC, unambiguously, so a model reading two runs' logs can compare
        // them without knowing which machine rendered which.
        assert_eq!(columns[1].len(), 20, "an ISO-8601 UTC instant: {line}");
        assert!(columns[1].ends_with('Z'), "{line}");
        assert_eq!(columns[2], "Fixture Author");
        // The author's name and **not** the email: a model does not need it,
        // and it would be needless personal data on an append-only chain.
        assert!(
            !line.contains("fixture@example.invalid"),
            "an author's email must never reach the chain: {line}"
        );
    }
    assert!(
        f.log(1).expect("one commit is readable").lines().count() == 1,
        "the count is honoured: {log}"
    );
}

#[test]
fn git_log_returns_only_a_commits_summary_line_not_its_body() {
    let f = committed();
    f.write("second.txt", "two\n").stage("second.txt").commit(
        "fix the thing\n\nA long explanation nobody asked for, mentioning \
         BODY-ONLY-MARKER and going on at length about why.\n",
    );

    let log = f.log(1).expect("the commit is readable");

    assert!(log.ends_with("fix the thing"), "{log}");
    // The summary is the first line by definition, which is what bounds this
    // tool's output to `count` short lines. A body reaches the prompt *and* the
    // Ledger, and commit bodies are unbounded.
    assert!(
        !log.contains("BODY-ONLY-MARKER"),
        "a commit body must not reach the model: {log}"
    );
}

#[test]
fn git_log_refuses_a_count_over_the_cap_and_names_the_cap() {
    let f = committed();

    let refusal = f
        .log(LOG_COUNT_CAP + 1)
        .expect_err("a count over the cap must be refused");

    // Refused rather than truncated, unlike `git_status`: there *is* a smaller
    // call for the model to make here, so naming the cap lets it make one —
    // `fs_read`'s reasoning exactly.
    assert!(
        refusal.contains(&LOG_COUNT_CAP.to_string()),
        "the refusal must name the cap so the model can act on it: {refusal}"
    );
    assert!(
        f.log(LOG_COUNT_CAP).is_ok(),
        "the cap itself must be allowed"
    );
}

#[test]
fn git_log_refuses_a_count_of_zero() {
    let f = committed();

    let refusal = f.log(0).expect_err("zero commits is not a question");
    assert!(!refusal.is_empty(), "the refusal must say something");
}

#[test]
fn git_log_on_a_repository_with_no_commits_is_a_tool_error_not_a_panic() {
    let f = unborn();

    let refusal = f
        .log(5)
        .expect_err("a repository with no commits has no log");

    // libgit2 says `reference 'refs/heads/work' not found`, which is true and
    // tells a model nothing it can use.
    assert!(
        refusal.contains("no commits"),
        "the refusal must say what is actually the matter: {refusal}"
    );
}

// ---------------------------------------------------------------------------
// SC-004 — the containment refusals, reached through the layer the model
// actually touches, and the two gates agreeing.
// ---------------------------------------------------------------------------

#[test]
fn a_root_inside_a_repository_is_refused_by_the_server_and_leaks_nothing() {
    let dir = TempDir::new().expect("a temp dir");
    let repo_path = dir.path().join("repo");
    std::fs::create_dir(&repo_path).expect("the repository directory is created");
    let repo = Repository::init(&repo_path).expect("a repository is initialised");
    repo.set_head(&format!("refs/heads/{BRANCH}"))
        .expect("HEAD names the fixture's branch");
    std::fs::write(repo_path.join("tracked.txt"), "parent contents\n").expect("a file to commit");
    let mut index = repo.index().expect("the index opens");
    index
        .add_path(Path::new("tracked.txt"))
        .expect("the path is staged");
    index.write().expect("the index is written");
    let tree = repo
        .find_tree(index.write_tree().expect("a tree"))
        .expect("the tree is found");
    let who = Signature::now("Parent Author", "parent@example.invalid").expect("a signature");
    repo.commit(Some("HEAD"), &who, &who, "PARENT-ONLY-SUMMARY", &tree, &[])
        .expect("the commit is written");

    let sub = repo_path.join("sub");
    std::fs::create_dir(&sub).expect("a subdirectory of the repository");
    let root = FsRoot::new(&sub).expect("a canonicalizable root");
    let server = EmbeddedServer::new(root);

    // The refusal happens at the layer the model reaches, not only at
    // `is_git_repository` — and the two must agree, because one is the server's
    // capability gate and the other is the CLI allowlist's.
    let status = server
        .git_status()
        .expect_err("a root inside a repository must be refused by the tool too");
    let log = server
        .git_log(Parameters(LogParams { count: 5 }))
        .expect_err("both tools refuse, not just one");
    assert!(
        !is_git_repository(&FsRoot::new(&sub).expect("a canonicalizable root")),
        "the gate and the tools must not be able to disagree"
    );

    for refusal in [&status, &log] {
        assert!(
            refusal.contains("not a git repository"),
            "the refusal must say why: {refusal}"
        );
        // Nothing of the enclosing repository may appear: not its branch, not
        // its commit summary, not its author. A refusal that leaked what it
        // refused to read would be the escape wearing a refusal's clothes.
        assert!(
            !refusal.contains("PARENT-ONLY-SUMMARY")
                && !refusal.contains(BRANCH)
                && !refusal.contains("Parent Author"),
            "the enclosing repository must appear nowhere in the refusal: {refusal}"
        );
    }
}

#[test]
fn a_repository_whose_worktree_is_outside_the_root_is_refused_by_the_server() {
    let dir = TempDir::new().expect("a temp dir");
    let root_path = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&root_path).expect("the root is created");
    std::fs::create_dir(&outside).expect("the sibling outside the root is created");
    std::fs::write(outside.join("OUTSIDE-ONLY.txt"), "not yours\n")
        .expect("a file outside the root");
    let repo = Repository::init(&root_path).expect("a repository is initialised");
    let target = std::fs::canonicalize(&outside)
        .expect("the sibling canonicalizes")
        .to_string_lossy()
        .replace('\\', "/");
    repo.config()
        .expect("the repository config opens")
        .set_str("core.worktree", target.trim_start_matches("//?/"))
        .expect("core.worktree is set");
    let server = EmbeddedServer::new(FsRoot::new(&root_path).expect("a canonicalizable root"));

    let refusal = server
        .git_status()
        .expect_err("a worktree outside the root must be refused");

    assert!(refusal.contains("outside the configured root"), "{refusal}");
    assert!(
        !refusal.contains("OUTSIDE-ONLY.txt"),
        "no file outside the root may appear anywhere: {refusal}"
    );
}

// ---------------------------------------------------------------------------
// SC-009 — the injection boundary. The only model-supplied value in the whole
// slice is a `u32`, and there is no command line for it to reach.
// ---------------------------------------------------------------------------

#[test]
fn a_count_that_is_not_a_number_is_refused_by_deserialization() {
    let crafted = serde_json::json!({"count": "5 --upload-pack=touch pwned"});

    let refusal = serde_json::from_value::<LogParams>(crafted)
        .expect_err("a string where a u32 belongs must not deserialize");

    // The positive half of the injection claim. `git_status` takes no
    // arguments at all and `git_log` takes one `u32`, so the crafted value
    // never becomes a value the tool can see — and even if it had, there is no
    // subprocess, no argument vector and no shell anywhere in this slice for it
    // to become command structure in. rmcp turns this class of failure into
    // `isError: true` rather than a protocol error, which is why
    // `governed_git_run.rs` can assert the run *survives* it.
    // Serde reports the *type* it wanted and not the field it wanted it for —
    // measured: `invalid type: string "…", expected u32`. Naming the type is
    // what lets a model correct itself, so that is what is asserted.
    assert!(
        refusal.to_string().contains("expected u32"),
        "the refusal must say what was expected: {refusal}"
    );
}
