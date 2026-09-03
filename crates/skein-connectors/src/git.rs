//! Reading one git repository, and the rule that it is the one the operator
//! named.
//!
//! **This is the only module in the workspace that names `git2`.** The boundary
//! is deliberate: `gix` was rejected on shipped footprint (112 packages for
//! `status` + `revision`, against `git2`'s four) rather than on quality, and if
//! that footprint ever stops mattering the swap touches this file and nothing
//! else. `skein-cli` asks [`is_git_repository`] and never sees a `git2` type.
//!
//! **No subprocess is spawned here, and there is no argument vector anywhere.**
//! That is not tidiness. A fixed `git` argv would still not have been safe: a
//! target repository's own `.git/config` can name `core.fsmonitor`, which
//! `git status` **executes** — measured — and which libgit2 reads and does not
//! run. Nor would it have been contained: `git -C <dir>` discovers upward, so a
//! root inside a repository silently reports on the enclosing one.

use crate::fs::FsRoot;
use crate::server::STATUS_ENTRY_CAP;
use chrono::DateTime;
use git2::{ErrorCode, Oid, Repository, Sort, Status, StatusOptions, Time};

/// The one door into a repository. Every git tool starts here.
///
/// `Repository::open` and **never** `discover`, `open_ext` or
/// `open_from_env`: `open` refuses a directory that is not itself a repository,
/// where the other three walk up the tree and would silently open a repository
/// the operator never named. `tests/git_root.rs` asserts `discover`'s
/// contrasting behaviour explicitly, so a helpful refactor to it becomes a
/// failing test rather than a review miss.
///
/// The worktree comparison at the end is not defensive padding — it closes a
/// measured escape. A repository's own `.git/config` may set `core.worktree`,
/// and such a repository opens fine at the named root while reporting, and
/// reporting *on*, a worktree somewhere else entirely. Comparing
/// `repo.workdir()` against the root is what refuses it; comparing the path
/// passed to `open` would not have noticed. Both sides are canonicalized, so on
/// Windows the comparison is verbatim against verbatim — the same reasoning
/// [`FsRoot::new`]'s docstring records.
///
/// Every refusal is an `Err(String)`, which rmcp turns into `isError: true`: a
/// tool error the model is told about and can act on, never a transport failure
/// that ends the run.
///
/// **Opened per call, never held in the server.** `git2::Repository` is not
/// `Sync` and rmcp's handler must be `Clone + Send + Sync + 'static`, so
/// opening per call sidesteps that entirely — and it re-verifies containment on
/// every call instead of caching a handle across a configuration change.
///
/// Slice 016's TOCTOU residual is inherited unchanged: a directory swapped
/// between `FsRoot::new`'s `canonicalize` and the `open` below escapes the root.
pub(crate) fn open_contained(root: &FsRoot) -> std::result::Result<Repository, String> {
    let repo = Repository::open(root.path()).map_err(|e| {
        format!(
            "{} is not a git repository: {}",
            root.path().display(),
            e.message()
        )
    })?;
    // `workdir()` is `None` for a bare repository and only for one, so this is
    // the bare refusal rather than a second check next to it.
    let Some(workdir) = repo.workdir() else {
        return Err(format!(
            "{} is a bare repository and has no working tree to report on",
            root.path().display()
        ));
    };
    let workdir = std::fs::canonicalize(workdir).map_err(|e| {
        format!(
            "{}: its working tree does not resolve: {e}",
            root.path().display()
        )
    })?;
    if workdir != root.path() {
        return Err(format!(
            "{} is a git repository whose working tree is {} — outside the configured root — and \
             is refused",
            root.path().display(),
            workdir.display()
        ));
    }
    Ok(repo)
}

/// True when [`open_contained`] succeeds, and nothing more.
///
/// Public because two gates need it and **neither may see `git2`**:
/// [`crate::EmbeddedServer::new`] disables the git routes when it is false, and
/// `skein-cli`'s `wiring::ToolArgs` omits the git names from its allowlist in
/// the same case. Both gates are required — an allowlisted name whose route is
/// disabled reaches the model as a transport error, which ends the run.
///
/// Delegating rather than re-deriving is the point: the two gates and the tools
/// cannot disagree about what a servable repository is, because there is one
/// answer and this is it.
pub fn is_git_repository(root: &FsRoot) -> bool {
    open_contained(root).is_ok()
}

/// Porcelain-v1 status for the one contained repository.
///
/// The shape is `git status --porcelain -b`'s own, because a model has seen
/// millions of lines of it and none of a prose paraphrase. Entries are sorted
/// **by path** before they are formatted, so the same worktree reads the same
/// way twice (`fs_list`'s precedent) — sorting the formatted lines instead
/// would order by the two-character code, which is not information anybody
/// asked to sort on.
///
/// The walk is bounded the way `git status --porcelain` bounds its own:
/// untracked directories collapse to one entry rather than recursing, and
/// ignored files are not reported at all. Rename detection stays off
/// (libgit2's default), so a rename appears as a delete plus an add — said in
/// the tool description rather than configured on, because detection is a
/// similarity search over the whole diff.
pub(crate) fn status(root: &FsRoot) -> std::result::Result<String, String> {
    let repo = open_contained(root)?;
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .include_unmodified(false);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|e| format!("the working tree is unreadable: {}", e.message()))?;

    let mut entries: Vec<(String, &'static str)> = statuses
        .iter()
        // `path_bytes` and not `path`: the latter is `None` for a path that is
        // not UTF-8, and dropping a changed file out of a status report is a
        // wrong answer in a right answer's shape. A lossy rendering still says
        // a file changed and says roughly which.
        .map(|entry| {
            (
                String::from_utf8_lossy(entry.path_bytes()).into_owned(),
                porcelain_code(entry.status()),
            )
        })
        .collect();
    entries.sort();

    let mut lines = vec![head_line(&repo)?];
    if entries.is_empty() {
        lines.push("# working tree clean".to_string());
    } else {
        let dropped = entries.len().saturating_sub(STATUS_ENTRY_CAP);
        lines.extend(
            entries
                .into_iter()
                .take(STATUS_ENTRY_CAP)
                .map(|(path, code)| format!("{code}\t{path}")),
        );
        // Truncated **and labelled**, where `fs_read` refuses an oversized
        // file. The asymmetry is deliberate: `git_status` takes no arguments,
        // so there is no smaller call for a model to make and a refusal would
        // leave a dirty repository permanently unreadable.
        if dropped > 0 {
            lines.push(format!(
                "# {dropped} more entries not shown, over the {STATUS_ENTRY_CAP}-entry cap"
            ));
        }
    }
    Ok(lines.join("\n"))
}

/// The newest `count` commits, newest first, one line each.
///
/// **The summary and never the body.** A summary is the first line by
/// definition, which is what bounds this tool's output to `count` short lines;
/// a body is unbounded and would reach the next turn's prompt *and* the Ledger.
/// **The author's name and never the email**, for the same reason in the other
/// direction: a model does not need it, and it would be needless personal data
/// on an append-only chain.
///
/// `count` arrives already checked against [`crate::LOG_COUNT_CAP`] — the cap
/// lives next to `fs_read`'s in `server.rs`, because both are the same kind of
/// decision about how much of a repository may become prompt.
pub(crate) fn log(root: &FsRoot, count: u32) -> std::result::Result<String, String> {
    let repo = open_contained(root)?;
    // Checked before the walk, because `push_head` on an unborn branch fails
    // with `reference 'refs/heads/master' not found` — true, and useless to a
    // model that only wants to know there is no history yet.
    if let Err(e) = repo.head() {
        return Err(if e.code() == ErrorCode::UnbornBranch {
            "the repository has no commits yet".to_string()
        } else {
            format!("the repository's HEAD is unreadable: {}", e.message())
        });
    }
    let mut walk = repo
        .revwalk()
        .map_err(|e| format!("the history is unreadable: {}", e.message()))?;
    // `TIME` alone is a date-ordered priority queue, and its tie-break among
    // commits sharing a second is arbitrary — measured: three fixture commits
    // written in the same second came back parent-before-child. `TOPOLOGICAL`
    // adds the constraint that a parent never precedes its child, which is
    // what "newest first" means to whoever reads the output.
    walk.set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .map_err(|e| format!("the history is unreadable: {}", e.message()))?;
    walk.push_head()
        .map_err(|e| format!("the history is unreadable: {}", e.message()))?;

    let mut lines = Vec::new();
    for oid in walk.take(count as usize) {
        let oid = oid.map_err(|e| format!("the history is unreadable: {}", e.message()))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| format!("commit {oid} is unreadable: {}", e.message()))?;
        let when = utc(commit.time())
            .ok_or_else(|| format!("commit {oid} is dated outside representable time"))?;
        lines.push(format!(
            "{}\t{when}\t{}\t{}",
            short(oid),
            String::from_utf8_lossy(commit.author().name_bytes()),
            commit
                .summary_bytes()
                .map(String::from_utf8_lossy)
                .unwrap_or_default()
        ));
    }
    Ok(lines.join("\n"))
}

/// Porcelain's `## <branch>` line, and a *named* state where there is no
/// branch to name.
///
/// An unborn branch and a detached HEAD are both states a repository is
/// legitimately in, not failures: a coding agent just handed a freshly
/// initialised repository needs to be told that, not handed libgit2's
/// `reference 'refs/heads/master' not found`.
fn head_line(repo: &Repository) -> std::result::Result<String, String> {
    match repo.head() {
        Ok(head) if head.is_branch() => Ok(format!(
            "## {}",
            String::from_utf8_lossy(head.shorthand_bytes())
        )),
        Ok(head) => {
            let commit = head
                .peel_to_commit()
                .map_err(|e| format!("the repository's HEAD is unreadable: {}", e.message()))?;
            Ok(format!("## (detached HEAD at {})", short(commit.id())))
        }
        Err(e) if e.code() == ErrorCode::UnbornBranch => {
            Ok(format!("## (unborn branch {})", unborn_branch(repo)?))
        }
        Err(e) => Err(format!(
            "the repository's HEAD is unreadable: {}",
            e.message()
        )),
    }
}

/// The branch `HEAD` points at when that branch has no commit yet.
///
/// `repo.head()` cannot answer, because the reference it would return does not
/// exist; `HEAD` itself does, and it is symbolic.
fn unborn_branch(repo: &Repository) -> std::result::Result<String, String> {
    let head = repo
        .find_reference("HEAD")
        .map_err(|e| format!("the repository's HEAD is unreadable: {}", e.message()))?;
    let target = head
        .symbolic_target_bytes()
        .ok_or_else(|| "the repository's HEAD names no branch".to_string())?;
    let target = String::from_utf8_lossy(target);
    Ok(target
        .strip_prefix("refs/heads/")
        .unwrap_or(&target)
        .to_string())
}

/// Porcelain v1's two-character `XY` code: `X` the index against `HEAD`, `Y`
/// the working tree against the index.
///
/// The mapping is written out rather than derived, because libgit2's `Status`
/// is a bitset in which several bits are set at once and porcelain shows
/// exactly one character per column. The order of the arms **is** the
/// precedence.
fn porcelain_code(status: Status) -> &'static str {
    if status.is_conflicted() {
        return "UU";
    }
    // A file the index has never heard of has no index column to report, which
    // is why porcelain spends both characters on saying so.
    if status == Status::WT_NEW {
        return "??";
    }
    match (index_code(status), worktree_code(status)) {
        ('A', ' ') => "A ",
        ('A', 'M') => "AM",
        ('A', 'D') => "AD",
        ('A', 'T') => "AT",
        ('M', ' ') => "M ",
        ('M', 'M') => "MM",
        ('M', 'D') => "MD",
        ('M', 'T') => "MT",
        ('D', ' ') => "D ",
        ('D', 'M') => "DM",
        ('R', ' ') => "R ",
        ('R', 'M') => "RM",
        ('R', 'D') => "RD",
        ('R', 'T') => "RT",
        ('T', ' ') => "T ",
        ('T', 'M') => "TM",
        ('T', 'D') => "TD",
        (' ', 'M') => " M",
        (' ', 'D') => " D",
        (' ', 'T') => " T",
        (' ', 'R') => " R",
        _ => "  ",
    }
}

fn index_code(status: Status) -> char {
    if status.is_index_new() {
        'A'
    } else if status.is_index_modified() {
        'M'
    } else if status.is_index_deleted() {
        'D'
    } else if status.is_index_renamed() {
        'R'
    } else if status.is_index_typechange() {
        'T'
    } else {
        ' '
    }
}

fn worktree_code(status: Status) -> char {
    if status.is_wt_modified() {
        'M'
    } else if status.is_wt_deleted() {
        'D'
    } else if status.is_wt_typechange() {
        'T'
    } else if status.is_wt_renamed() {
        'R'
    } else {
        ' '
    }
}

/// The seven-hex prefix `git log --oneline` chose, and the one a model expects
/// to be able to paste back.
fn short(oid: Oid) -> String {
    oid.to_string()[..7].to_string()
}

/// The commit's instant in **UTC**, so two runs' logs compare without knowing
/// which machine rendered which. `git2::Time` carries the author's own offset;
/// this deliberately discards it rather than printing a local clock a model
/// cannot interpret.
fn utc(time: Time) -> Option<String> {
    DateTime::from_timestamp(time.seconds(), 0)
        .map(|instant| instant.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}
