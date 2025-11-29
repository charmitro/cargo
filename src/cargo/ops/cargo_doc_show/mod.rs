//! Implementation of `cargo doc --show` for terminal documentation display.
//!
//! This module provides functionality to display documentation in the terminal,
//! similar to `go doc`. It uses rustdoc's JSON output format to parse and display
//! documentation for crates and their items.

mod json_parser;
mod query;
mod render;

use crate::core::Workspace;
use crate::ops::{self, CompileOptions, DocOptions, OutputFormat};
use crate::util::CargoResult;
use std::path::PathBuf;

/// Options for the `cargo doc --show` command.
#[derive(Debug)]
pub struct DocShowOptions {
    /// The item path to show documentation for.
    /// `None` means show crate root documentation.
    /// `Some(path)` means show documentation for the specified item path (e.g., "HashMap::insert").
    pub item: Option<String>,
    /// Compilation options inherited from the doc command.
    pub compile_opts: CompileOptions,
}

/// Main entry point for `cargo doc --show`.
///
/// This function generates rustdoc JSON output (if needed), parses it,
/// finds the requested item, and renders its documentation to the terminal.
pub fn doc_show(ws: &Workspace<'_>, opts: &DocShowOptions) -> CargoResult<()> {
    // Generate rustdoc JSON output
    let json_path = generate_json(ws, &opts.compile_opts)?;

    // Parse the JSON and find the requested item
    let doc_data = json_parser::parse_json(&json_path)?;
    let item = query::find_item(&doc_data, opts.item.as_deref())?;

    // Render to terminal
    render::display(&item, ws.gctx())?;

    Ok(())
}

/// Generate rustdoc JSON output for the workspace.
fn generate_json(ws: &Workspace<'_>, compile_opts: &CompileOptions) -> CargoResult<PathBuf> {
    // Create doc options with JSON output format
    let doc_opts = DocOptions {
        open_result: false,
        output_format: OutputFormat::Json,
        compile_opts: compile_opts.clone(),
    };

    // Run the doc command to generate JSON
    ops::doc(ws, &doc_opts)?;

    // Find the generated JSON file
    let pkg = ws.current()?;
    let crate_name = pkg.name().replace('-', "_");
    let target_dir = ws.target_dir();

    // JSON output is in target/doc/<crate_name>.json
    let json_path = target_dir
        .join("doc")
        .join(format!("{}.json", crate_name))
        .into_path_unlocked();

    if !json_path.exists() {
        anyhow::bail!(
            "rustdoc JSON output not found at {}\n\
             Note: `cargo doc --show` requires nightly Rust for rustdoc JSON output.\n\
             Try: `cargo +nightly doc --show`",
            json_path.display()
        );
    }

    Ok(json_path)
}
