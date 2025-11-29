use crate::command_prelude::*;

use cargo::ops::{self, DocOptions, DocShowOptions};

pub fn cli() -> Command {
    subcommand("doc")
        // subcommand aliases are handled in aliased_command()
        // .alias("d")
        .about("Build a package's documentation")
        .arg(flag(
            "open",
            "Opens the docs in a browser after the operation",
        ))
        .arg(flag(
            "no-deps",
            "Don't build documentation for dependencies",
        ))
        .arg(flag("document-private-items", "Document private items"))
        .arg(
            Arg::new("show")
                .long("show")
                .value_name("ITEM")
                .help("Display documentation in terminal (unstable, requires -Z unstable-options)")
                .num_args(0..=1)
                .default_missing_value("")
                .conflicts_with("open"),
        )
        .arg_message_format()
        .arg_silent_suggestion()
        .arg_package_spec(
            "Package to document",
            "Document all packages in the workspace",
            "Exclude packages from the build",
        )
        .arg_features()
        .arg_targets_lib_bin_example(
            "Document only this package's library",
            "Document only the specified binary",
            "Document all binaries",
            "Document only the specified example",
            "Document all examples",
        )
        .arg_parallel()
        .arg_release("Build artifacts in release mode, with optimizations")
        .arg_profile("Build artifacts with the specified profile")
        .arg_target_triple("Build for the target triple")
        .arg_target_dir()
        .arg_unit_graph()
        .arg_timings()
        .arg_manifest_path()
        .arg_lockfile_path()
        .arg_ignore_rust_version()
        .after_help(color_print::cstr!(
            "Run `<bright-cyan,bold>cargo help doc</>` for more detailed information.\n"
        ))
}

pub fn exec(gctx: &mut GlobalContext, args: &ArgMatches) -> CliResult {
    let ws = args.workspace(gctx)?;

    // Handle --show flag for terminal documentation display
    if let Some(item) = args.get_one::<String>("show") {
        gctx.cli_unstable().fail_if_stable_opt("--show", 0)?;
        let intent = UserIntent::Doc {
            deps: false, // Don't need deps for showing
            json: true,  // Need JSON output
        };
        let mut compile_opts =
            args.compile_options(gctx, intent, Some(&ws), ProfileChecking::Custom)?;
        compile_opts.rustdoc_document_private_items = args.flag("document-private-items");

        let show_opts = DocShowOptions {
            item: if item.is_empty() {
                None
            } else {
                Some(item.clone())
            },
            compile_opts,
        };
        ops::doc_show(&ws, &show_opts)?;
        return Ok(());
    }

    let intent = UserIntent::Doc {
        deps: !args.flag("no-deps"),
        json: false,
    };
    let mut compile_opts =
        args.compile_options(gctx, intent, Some(&ws), ProfileChecking::Custom)?;
    compile_opts.rustdoc_document_private_items = args.flag("document-private-items");

    let doc_opts = DocOptions {
        open_result: args.flag("open"),
        output_format: ops::OutputFormat::Html,
        compile_opts,
    };
    ops::doc(&ws, &doc_opts)?;
    Ok(())
}
