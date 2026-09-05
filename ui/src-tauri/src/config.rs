//! Where the desktop app gets the flags it launches `skein acp-agent` with.
//!
//! v0 has no settings screen (that is a follow-up slice), and it has no config
//! file either — for the same reason `SiloArgs::root` refuses to guess one:
//! guessing would put an agent's journal somewhere the operator did not name.
//! So the window reads the same environment the CLI already documents, and
//! refuses to start when a required variable is absent, naming it.
//!
//! Nothing here invents a configuration surface. Every value below becomes a
//! flag `skein acp-agent` already parses (`crates/skein-cli/src/main.rs`).

use crate::session::AgentLaunch;
use std::path::{Path, PathBuf};

/// The `skein` binary, overridable for a developer running an unbundled build.
const BIN: &str = "SKEIN_UI_BIN";
/// The silo root, exactly as `skein --root` / `$SKEIN_ROOT` means it.
const ROOT: &str = "SKEIN_ROOT";
/// Which silo the window's sessions land in.
const SILO: &str = "SKEIN_UI_SILO";
/// The model name. `--model` has no default in the CLI and gets none here.
const MODEL: &str = "SKEIN_UI_MODEL";
/// The provider base URL. `skein-gateway` can only reach loopback either way.
const BASE_URL: &str = "SKEIN_MODEL_BASE_URL";
/// The one directory an agent may work in. Absent means the session has no
/// tools at all — `crates/skein-cli/src/wiring.rs`'s "no root, no tools".
const FS_ROOT: &str = "SKEIN_UI_FS_ROOT";

/// The default silo, so a first run needs two variables rather than three.
const DEFAULT_SILO: &str = "ui";

/// Reads `env` and builds the child's launch, or explains what is missing.
///
/// `exe_dir` is where the app's own executable lives; the `skein` binary is
/// expected beside it, which is what a bundled app ships and what a `cargo
/// build` layout already produces. Never a hardcoded developer path.
pub fn launch_from_env(
    env: impl Fn(&str) -> Option<String>,
    exe_dir: &Path,
) -> Result<AgentLaunch, String> {
    let required = |name: &str| {
        env(name)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("{name} is not set: the window cannot guess it"))
    };

    let binary = match env(BIN) {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => exe_dir.join(format!("skein{}", std::env::consts::EXE_SUFFIX)),
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

    // The session's working directory is the one the agent may touch when there
    // is one, and the silo root otherwise.
    let cwd = fs_root.map_or_else(|| PathBuf::from(&root), PathBuf::from);
    Ok(AgentLaunch::new(binary).args(args).cwd(cwd))
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
    fn argv(launch: &AgentLaunch) -> Vec<String> {
        launch.arguments().to_vec()
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
            launch.command(),
            Path::new("/app").join(format!("skein{}", std::env::consts::EXE_SUFFIX))
        );
    }

    #[test]
    fn an_explicit_binary_overrides_the_default() {
        let mut pairs = minimal();
        pairs.push((BIN, "/opt/skein-dev"));
        let launch = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert_eq!(launch.command(), Path::new("/opt/skein-dev"));
    }

    #[test]
    fn the_session_directory_is_the_fs_root_when_there_is_one_and_the_silo_root_otherwise() {
        let bare = launch_from_env(env_of(&minimal()), Path::new("/app")).expect("a launch");
        assert_eq!(bare.working_dir(), Path::new("/silos"));

        let mut pairs = minimal();
        pairs.push((FS_ROOT, "/work"));
        let scoped = launch_from_env(env_of(&pairs), Path::new("/app")).expect("a launch");
        assert_eq!(scoped.working_dir(), Path::new("/work"));
    }
}
