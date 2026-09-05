//! Tauri's own build step: it reads `tauri.conf.json`, generates the app's
//! capability/permission set, and (on Windows) the resource script.
//!
//! It also means `cargo build -p heddle-ui` needs `../dist` to exist, because
//! `tauri.conf.json`'s `frontendDist` points at it — run `npm run build` in
//! `ui/` first. `docs/UI.md` and `.github/workflows/core.yml` both say so.

fn main() {
    tauri_build::build()
}
