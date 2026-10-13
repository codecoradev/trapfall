//! Warn when the SvelteKit bundle has not been built.
//!
//! `web/build/` is embedded via rust-embed. Only a placeholder `index.html`
//! is tracked in git, so a fresh clone built without `npm run build` would
//! serve a dashboard whose JS/CSS are missing (blank page).

use std::path::Path;

fn main() {
    let app_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/build/_app");
    println!("cargo:rerun-if-changed=../../web/build/_app");
    if !app_dir.is_dir() {
        println!(
            "cargo:warning=web/build/_app not found: the embedded dashboard will be blank. \
             Run `cd web && npm ci && npm run build` before `cargo build`."
        );
    }
}
