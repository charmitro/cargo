//! Terminal rendering for documentation display.
//!
//! This module provides functionality to render documentation to the terminal
//! with colors and formatting, following the patterns from `cargo info`.

use std::io::Write;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::query::{FieldDoc, ItemDoc, MethodDoc, VariantDoc};
use crate::GlobalContext;
use crate::core::shell::Verbosity;
use crate::util::CargoResult;
use crate::util::style::{HEADER, LITERAL, WARN};

/// Display documentation for an item in the terminal.
pub fn display(item: &ItemDoc, gctx: &GlobalContext) -> CargoResult<()> {
    let header = HEADER;
    let literal = LITERAL;
    let warn = WARN;

    let mut shell = gctx.shell();
    let verbosity = shell.verbosity();
    let stdout = shell.out();

    // Kind and name header
    writeln!(
        stdout,
        "{header}{}{header:#} {literal}{}{literal:#}",
        item.kind, item.name
    )?;

    // Deprecation warning
    if let Some(ref dep) = item.deprecation {
        writeln!(stdout, "{warn}Deprecated: {}{warn:#}", dep)?;
    }

    // Signature
    writeln!(stdout)?;
    writeln!(stdout, "    {}", item.signature)?;

    // Documentation
    if let Some(ref docs) = item.docs {
        writeln!(stdout)?;
        render_markdown(docs, stdout)?;
    }

    // Fields (for structs)
    if !item.fields.is_empty() {
        writeln!(stdout)?;
        writeln!(stdout, "{header}Fields:{header:#}")?;
        render_fields(&item.fields, stdout)?;
    }

    // Variants (for enums)
    if !item.variants.is_empty() {
        writeln!(stdout)?;
        writeln!(stdout, "{header}Variants:{header:#}")?;
        render_variants(&item.variants, stdout)?;
    }

    // Methods
    if !item.methods.is_empty() {
        writeln!(stdout)?;
        writeln!(stdout, "{header}Methods:{header:#}")?;
        render_methods(&item.methods, stdout, verbosity)?;
    }

    Ok(())
}

/// Render markdown documentation to the terminal.
fn render_markdown(md: &str, out: &mut dyn Write) -> CargoResult<()> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(md, options);

    let mut in_code_block = false;
    let mut in_heading = false;
    let mut list_depth: usize = 0;
    let mut list_item_started = false;

    for event in parser {
        match event {
            Event::Start(Tag::Paragraph) => {
                // Nothing to do at start
            }
            Event::End(TagEnd::Paragraph) => {
                writeln!(out)?;
                writeln!(out)?;
            }

            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                write!(out, "{}", HEADER)?;
            }
            Event::End(TagEnd::Heading(_)) => {
                write!(out, "{:#}", HEADER)?;
                writeln!(out)?;
                writeln!(out)?;
                in_heading = false;
            }

            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                writeln!(out)?;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                writeln!(out)?;
            }

            Event::Start(Tag::List(_)) => {
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth -= 1;
                if list_depth == 0 {
                    writeln!(out)?;
                }
            }

            Event::Start(Tag::Item) => {
                let indent = "    ".repeat(list_depth.saturating_sub(1));
                write!(out, "{}  * ", indent)?;
                list_item_started = true;
            }
            Event::End(TagEnd::Item) => {
                writeln!(out)?;
                list_item_started = false;
            }

            Event::Start(Tag::Strong) => {
                write!(out, "{}", HEADER)?;
            }
            Event::End(TagEnd::Strong) => {
                write!(out, "{:#}", HEADER)?;
            }

            Event::Start(Tag::Emphasis) => {
                // Terminal doesn't have italics, use underscore
                write!(out, "_")?;
            }
            Event::End(TagEnd::Emphasis) => {
                write!(out, "_")?;
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                // We'll show the URL after the link text
                write!(out, "{}", LITERAL)?;
                // Store URL for later - simplified: just continue
                let _ = dest_url; // We'll handle this simply
            }
            Event::End(TagEnd::Link) => {
                write!(out, "{:#}", LITERAL)?;
            }

            Event::Code(code) => {
                write!(out, "{}{}{:#}", LITERAL, code, LITERAL)?;
            }

            Event::Text(text) => {
                if in_code_block {
                    // Indent code blocks
                    for line in text.lines() {
                        writeln!(out, "        {}", line)?;
                    }
                } else if in_heading {
                    write!(out, "{}", text.to_uppercase())?;
                } else {
                    write!(out, "{}", text)?;
                }
            }

            Event::SoftBreak => {
                if !list_item_started {
                    write!(out, " ")?;
                }
            }
            Event::HardBreak => {
                writeln!(out)?;
            }

            Event::Rule => {
                writeln!(out, "────────────────────────────────────────")?;
            }

            // Handle other events minimally
            _ => {}
        }
    }

    Ok(())
}

/// Render struct fields.
fn render_fields(fields: &[FieldDoc], out: &mut dyn Write) -> CargoResult<()> {
    for field in fields {
        write!(out, "    {}{}{:#}", LITERAL, field.name, LITERAL)?;
        writeln!(out, ": {}", field.type_str)?;

        if let Some(ref docs) = field.docs {
            if let Some(first_line) = docs.lines().find(|l| !l.trim().is_empty()) {
                writeln!(out, "        {}", first_line.trim())?;
            }
        }
    }
    Ok(())
}

/// Render enum variants.
fn render_variants(variants: &[VariantDoc], out: &mut dyn Write) -> CargoResult<()> {
    for variant in variants {
        writeln!(out, "    {}{}{:#}", LITERAL, variant.signature, LITERAL)?;

        if let Some(ref docs) = variant.docs {
            if let Some(first_line) = docs.lines().find(|l| !l.trim().is_empty()) {
                writeln!(out, "        {}", first_line.trim())?;
            }
        }
    }
    Ok(())
}

/// Render methods.
fn render_methods(
    methods: &[MethodDoc],
    out: &mut dyn Write,
    verbosity: Verbosity,
) -> CargoResult<()> {
    let max_methods = match verbosity {
        Verbosity::Verbose => usize::MAX,
        _ => 20,
    };

    let show_count = methods.len().min(max_methods);
    let remaining = methods.len().saturating_sub(max_methods);

    for method in methods.iter().take(show_count) {
        writeln!(out, "    {}", method.signature)?;

        if let Some(ref brief) = method.brief {
            writeln!(out, "        {}", brief)?;
        }
    }

    if remaining > 0 {
        let style = anstyle::Style::new() | anstyle::Effects::ITALIC;
        writeln!(
            out,
            "    {style}... and {} more methods (use -v for all){style:#}",
            remaining
        )?;
    }

    Ok(())
}
