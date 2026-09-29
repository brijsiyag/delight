//! Delight's development and release tasks, run as `cargo xtask <task>` (the alias is in
//! `.cargo/config.toml`).
//!
//! * `wasi-sdk`: fetch the pinned WASI SDK into `target/wasi-sdk`, checking its SHA-256.
//!   Building the built-in JSON and YAML plugins needs it (tree-sitter is C).
//!
//! The release tasks (version checks, bundling, signing, notarising) join it in step 15 of
//! `docs/plan.md`.

mod wasi_sdk;

use std::path::PathBuf;

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("wasi-sdk") => wasi_sdk::run(args.collect(), &root()),
        Some("help" | "--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => bail!("there is no task {other:?}\n\n{USAGE}"),
    }
}

const USAGE: &str = "cargo xtask <task>

Tasks:
  wasi-sdk [--force] [--to <folder>]
      Fetch the pinned WASI SDK into target/wasi-sdk (or <folder>), checking its SHA-256.
      Does nothing if that version is already there; --force fetches it again.";

/// The repository's root: this crate's folder is directly under it.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask is in the repository").to_path_buf()
}
