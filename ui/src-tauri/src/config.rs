//! Where the desktop app gets the flags it launches `heddle acp-agent` with.
//!
//! There is no config file — for the same reason `SiloArgs::root` refuses to
//! guess one: guessing would put an agent's journal somewhere the operator did
//! not name. So the window reads the same environment the CLI already
//! documents, and refuses to start when a required variable is absent, naming
//! it. The Settings screen slice 041 added *reports* this resolution; it does
//! not edit it, so this file is still the only place configuration is decided.
//!
//! Nothing here invents a configuration surface. Every value below becomes a
//! flag `heddle acp-agent` already parses (`crates/heddle-cli/src/main.rs`).

use crate::session::AgentLaunch;
use std::path::{Path, PathBuf};

/// The `heddle` binary, overridable for a developer running an unbundled build.
const BIN: &str = "HEDDLE_UI_BIN";
/// The silo root, exactly as `heddle --root` / `$HEDDLE_ROOT` means it.
const ROOT: &str = "HEDDLE_ROOT";
/// Which silo the window's sessions land in.
const SILO: &str = "HEDDLE_UI_SILO";
/// The model name. `--model` has no default in the CLI and gets none here.
const MODEL: &str = "HEDDLE_UI_MODEL";
/// The provider base URL. `heddle-gateway` can only reach loopback either way.
const BASE_URL: &str = "HEDDLE_MODEL_BASE_URL";
/// The one directory an agent may work in. Absent means the session has no
/// tools at all — `crates/heddle-cli/src/wiring.rs`'s "no root, no tools".
const FS_ROOT: &str = "HEDDLE_UI_FS_ROOT";
/// Whether the session may launch a sandboxed process. `--allow-run` is a
/// *second* opt-in on top of `--fs-root` in `RunArgs`, and it stays one here:
/// this variable alone grants nothing, and the child refuses the flag outright
/// off Windows, exactly as `RunArgs::resolve` does for the CLI.
const ALLOW_RUN: &str = "HEDDLE_UI_ALLOW_RUN";

/// The default silo, so a first run needs two variables rather than three.
const DEFAULT_SILO: &str = "ui";

/// One environment, resolved once: the child's launch **and** the values that
/// launch was built from.
///
/// The second half is what the Settings screen reports and what the Code view
/// browses. Keeping them on the same value as the `AgentLaunch` is the point:
/// a screen that re-read `std::env` later could report a root the running child
/// was never given, and [`heddle_connectors::FsRoot`]'s own docstring names
/// exactly that class of drift as the reason it pins a directory handle rather
/// than re-walking a name.
#[derive(Clone, Debug)]
pub struct ResolvedLaunch {
    /// The child process to spawn, argv and all.
    pub launch: AgentLaunch,
    /// The one directory the session may work in, or `None` for no tools.
    pub fs_root: Option<PathBuf>,
    /// Whether `--allow-run` was passed, i.e. whether `proc_run` was offered.
    pub allow_run: bool,
}

/// Reads `env` and builds the child's launch, or explains what is missing.
///
/// `exe_dir` is where the app's own executable lives; the `heddle` binary is
/// expected beside it, which is what a bundled app ships and what a `cargo
/// build` layout already produces. Never a hardcoded developer path.
pub fn launch_from_env(
    env: impl Fn(&str) -> Option<String>,
    exe_dir: &Path,
) -> Result<ResolvedLaunch, String> {
    let required = |name: &str| {
        env(name)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("{name} is not set: the window cannot guess it"))
    };

    let binary = match env(BIN) {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => exe_dir.join(format!("heddle{}", std::env::consts::EXE_SUFFIX)),
    };
    let root = required(ROOT)?;
    let model = required(MODEL)?;
    let silo = env(SILO)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SILO.to_string());

    let mut args = vec![
        "acp-agent".to_string(),
        "--root".to_string(),
        root.clone(),
        "--silo".to_string(),
        silo,
        "--model".to_string(),
        model,
    ];
    // Passed as a flag when it is set, and simply absent otherwise: the child
    // inherits this process's environment, so an unset value here is not the
    // same as an unset value there.
    if let Some(base_url) = env(BASE_URL).filter(|value| !value.trim().is_empty()) {
        args.push("--base-url".to_string());
        args.push(base_url);
    }
    let fs_root = env(FS_ROOT).filter(|value| !value.trim().is_empty());
    if let Some(fs_root) = &fs_root {
        args.push("--fs-root".to_string());
        args.push(fs_root.clone());
    }
    // Gated on `--fs-root` before it is even read, because `RunArgs` gates it
    // that way: `proc_run` is offered over the root or not at all. Passing
    // `--allow-run` with no root would hand the child a flag it can only
    // resolve into a capability it has nowhere to apply.
    let allow_run = fs_root.is_some() && flag(&env, ALLOW_RUN)?;
    if allow_run {
        args.push("--allow-run".to_string());
    }

    // The session's working directory is the one the agent may touch when there
    // is one, and the silo root otherwise.
    let cwd = fs_root
        .as_ref()
        .map_or_else(|| PathBuf::from(&root), PathBuf::from);
    Ok(ResolvedLaunch {
        launch: AgentLaunch::new(binary).args(args).cwd(cwd),
        fs_root: fs_root.map(PathBuf::from),
        allow_run,
    })
}

/// A boolean variable, or a refusal that names it.
///
/// Unset and blank are off, matching every optional variable above. Anything
/// that is neither a recognised yes nor a recognised no is an **error** rather
/// than a silent off: an operator who wrote `HEDDLE_UI_ALLOW_RUN=maybe` asked
/// for something, and quietly reading it as "no" would be the window guessing —
/// the one thing this file exists not to do.
fn flag(env: &impl Fn(&str) -> Option<String>, name: &str) -> Result<bool, String> {
    match env(name) {
        None => Ok(false),
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "0" | "false" | "no" => Ok(false),
            "1" | "true" | "yes" => Ok(true),
            other => Err(format!(
                "{name} is {other:?}: write 1/true/yes or 0/false/no, or leave it unset"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    fn minimal() -> Vec<(&'static str, &'static str)> {
        vec![(ROOT, "/silos"), (MODEL, "llama3.1")]
    }

    /// Asserts on the argv the child will really receive, in order.
    fn argv(resolved: &ResolvedLaunch) -> Vec<String> {
        resolved.launch.arguments().to_vec()
    }

    #[test]
    fn a_missing_root_names_the_variable_instead_of_guessing_one() {
        let error = launch_from_env(env_of(&[(MODEL, "llama3.1")]), Path::new("/app"))
            .expect_err("a launch with no silo root must be refused");
        assert!(
            error.contains(ROOT),
            "the refusal must name it, got {error:?}"
        );
    }

    #[test]
    fn a_missing_model_names_the_variable_because_the_cli_has_no_default() {
        let error = launch_from_env(env_of(&[(ROOT, "/silos")]), Path::new("/app"))
            .expect_err("a launch with no model must be refused");
        assert!(
            error.contains(MODEL),
            "the refusal must name it, got {error:?}"
        );
    }

    #[test]
    fn a_blank_value_counts_as_unset() {
        let error = launch_from_env(
            env_of(&[(ROOT, "/silos"), (MODEL, "   ")]),
            Path::new("/app"),
        )
        .expect_err("a whitespace-only model must be refused");
        assert!(error.contains(MODEL), "got {error:?}");
    }

    #[test]
    fn the_minimal_environment_produces_the_documented_subcommand() {
        let launch =
            launch_from_env(env_of(&minimal()), Path::new("/app")).expect("a minimal launch");
        let argv = argv(&launch);
        for expected in ["acp-agent", "--root", "/silos", "--silo", "ui", "--model"] {
            assert!(
                argv.iter().any(|arg| arg == expected),
                "{expected:?} missing from {argv:?}"
            );
        }
    }

    #[test]
    fn optional_flags_are_absent_rather_than_empty_when_unset() {
        let launch =
            launch_from_env(env_of(&minimal()), Path::new("/app")).expect("a minimal launch");
        let argv = argv(&launch);
        for absent in ["--base-url", "--fs-root"] {
            assert!(
                !argv.iter().any(|arg| arg == absent),
                "{absent:?} must not appear when its variable is unset, got {argv:?}"
            );
        }
    }

    #[test]
    fn a_configured_fs_root_becomes_the_flag_that_grants_tools() {
        let mut pairs = minimal();
        pairs.push((FS_ROOT, "/work"));
        let launch = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        let argv = argv(&launch);
        assert!(argv.iter().any(|arg| arg == "--fs-root"));
        assert!(argv.iter().any(|arg| arg == "/work"));
    }

    #[test]
    fn the_binary_defaults_to_the_one_beside_the_app_not_to_a_developer_path() {
        let launch = launch_from_env(env_of(&minimal()), Path::new("/app")).expect("a launch");
        assert_eq!(
            launch.launch.command(),
            Path::new("/app").join(format!("heddle{}", std::env::consts::EXE_SUFFIX))
        );
    }

    #[test]
    fn an_explicit_binary_overrides_the_default() {
        let mut pairs = minimal();
        pairs.push((BIN, "/opt/heddle-dev"));
        let launch = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert_eq!(launch.launch.command(), Path::new("/opt/heddle-dev"));
    }

    #[test]
    fn the_resolved_launch_reports_the_same_root_it_put_on_the_argv() {
        let mut pairs = minimal();
        pairs.push((FS_ROOT, "/work"));
        let resolved = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert_eq!(resolved.fs_root, Some(PathBuf::from("/work")));

        let bare = launch_from_env(env_of(&minimal()), Path::new("/app")).expect("a launch");
        assert_eq!(
            bare.fs_root, None,
            "no HEDDLE_UI_FS_ROOT means no root to report, not an empty one"
        );
    }

    #[test]
    fn allow_run_is_off_unless_the_operator_turns_it_on() {
        let mut pairs = minimal();
        pairs.push((FS_ROOT, "/work"));
        let off = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert!(!off.allow_run);
        assert!(!argv(&off).iter().any(|arg| arg == "--allow-run"));

        pairs.push((ALLOW_RUN, "true"));
        let on = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert!(on.allow_run);
        assert!(argv(&on).iter().any(|arg| arg == "--allow-run"));
    }

    #[test]
    fn allow_run_without_a_root_grants_nothing_because_proc_run_is_offered_over_the_root() {
        let mut pairs = minimal();
        pairs.push((ALLOW_RUN, "1"));
        let resolved = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert!(!resolved.allow_run);
        assert!(
            !argv(&resolved).iter().any(|arg| arg == "--allow-run"),
            "the child must not be handed a flag it has no root to apply, got {:?}",
            argv(&resolved)
        );
    }

    #[test]
    fn an_unreadable_allow_run_value_is_refused_by_name_rather_than_read_as_no() {
        let mut pairs = minimal();
        pairs.push((FS_ROOT, "/work"));
        pairs.push((ALLOW_RUN, "maybe"));
        let error = launch_from_env(env_of(&pairs), Path::new("/app"))
            .expect_err("an unrecognised boolean must be refused");
        assert!(error.contains(ALLOW_RUN), "got {error:?}");
    }

    #[test]
    fn the_session_directory_is_the_fs_root_when_there_is_one_and_the_silo_root_otherwise() {
        let bare = launch_from_env(env_of(&minimal()), Path::new("/app")).expect("a launch");
        assert_eq!(bare.launch.working_dir(), Path::new("/silos"));

        let mut pairs = minimal();
        pairs.push((FS_ROOT, "/work"));
        let scoped = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert_eq!(scoped.launch.working_dir(), Path::new("/work"));
    }
}
