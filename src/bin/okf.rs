//! `okf` — a command-line tool for the Open Knowledge Format.
//!
//! Subcommands:
//!   validate <bundle>   Check a bundle against OKF v0.1 §9 conformance.
//!   info     <bundle>   Print a summary of a bundle.
//!   index    <bundle>   (Re)generate every index.md in a bundle.
//!   graph    <bundle>   Print the cross-link graph (text, or DOT with --dot).
//!   parse    <file>     Parse one concept document and print its structure.
//!   fmt      <file>     Normalize a document by parse + re-serialize.
//!
//! Argument parsing is hand-rolled to keep the crate dependency-free.

use okf::{validate_bundle, Bundle, Document, Severity};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let cmd = args[0].as_str();
    let rest = &args[1..];

    let result = match cmd {
        "validate" => cmd_validate(rest),
        "info" => cmd_info(rest),
        "index" => cmd_index(rest),
        "graph" => cmd_graph(rest),
        "parse" => cmd_parse(rest),
        "fmt" => cmd_fmt(rest),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        "-V" | "--version" | "version" => {
            println!("okf {} (OKF spec v{})", env!("CARGO_PKG_VERSION"), okf::OKF_VERSION);
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("unknown subcommand: {other}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "\
okf — Open Knowledge Format toolkit

USAGE:
    okf <command> [args]

COMMANDS:
    validate <bundle>    Check a bundle against OKF v0.1 conformance (§9)
    info     <bundle>    Summarize a bundle (concepts, types, links, version)
    index    <bundle>    (Re)generate every index.md in the bundle
    graph    <bundle>    Print the cross-link graph (--dot for Graphviz DOT)
    parse    <file>      Parse one concept document and print its structure
    fmt      <file>      Normalize a document by parse + re-serialize (-w writes)

OPTIONS:
    -h, --help           Show this help
    -V, --version        Show version";

/// Returns the first positional argument, or an error. Everything after a `--`
/// separator is treated as positional (so paths beginning with `-` work).
fn positional<'a>(args: &'a [String], what: &str) -> Result<&'a str, String> {
    if let Some(pos) = args.iter().position(|a| a == "--") {
        if let Some(arg) = args.get(pos + 1) {
            return Ok(arg.as_str());
        }
    }
    args.iter()
        .find(|a| !a.starts_with('-'))
        .map(String::as_str)
        .ok_or_else(|| format!("missing {what}"))
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn load(path: &str) -> Result<Bundle, String> {
    Bundle::load(path).map_err(|e| e.to_string())
}

fn cmd_validate(args: &[String]) -> Result<ExitCode, String> {
    let path = positional(args, "<bundle>")?;
    let bundle = load(path)?;
    let report = validate_bundle(&bundle);

    for d in &report.diagnostics {
        println!("{d}");
    }

    let errors = report.error_count();
    let warnings = report.warning_count();
    let infos = report.of(Severity::Info).count();
    println!(
        "\n{} concept(s); {errors} error(s), {warnings} warning(s), {infos} info.",
        bundle.len()
    );

    if report.is_conformant() {
        println!("✓ conformant with OKF v{}", okf::OKF_VERSION);
        Ok(ExitCode::SUCCESS)
    } else {
        println!("✗ not conformant with OKF v{}", okf::OKF_VERSION);
        Ok(ExitCode::FAILURE)
    }
}

fn cmd_info(args: &[String]) -> Result<ExitCode, String> {
    let path = positional(args, "<bundle>")?;
    let bundle = load(path)?;

    println!("bundle:     {}", bundle.root().display());
    if let Some(v) = bundle.okf_version() {
        println!("okf_version: {v}");
    }
    println!("concepts:   {}", bundle.len());
    println!("index.md:   {}", bundle.index_files().len());
    println!("log.md:     {}", bundle.log_files().len());

    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    for c in bundle.concepts() {
        let t = c.document.frontmatter.type_().unwrap_or_else(|| "(none)".to_string());
        *by_type.entry(t).or_default() += 1;
    }
    if !by_type.is_empty() {
        println!("\ntypes:");
        for (t, n) in &by_type {
            println!("  {n:>4}  {t}");
        }
    }

    let broken = bundle.broken_links();
    let mut total_links = 0;
    for c in bundle.concepts() {
        total_links += bundle.links_from(&c.id).len();
    }
    println!("\nlinks:      {total_links} internal ({} broken)", broken.len());

    if !bundle.parse_errors().is_empty() {
        println!("\nunparseable files:");
        for (p, e) in bundle.parse_errors() {
            println!("  {}: {e}", p.display());
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_index(args: &[String]) -> Result<ExitCode, String> {
    let path = positional(args, "<bundle>")?;
    let written = okf::index::regenerate_indexes(path).map_err(|e| e.to_string())?;
    if written.is_empty() {
        println!("no index files written (empty bundle?)");
    } else {
        for p in &written {
            println!("wrote {}", p.display());
        }
        println!("\n{} index file(s) regenerated.", written.len());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_graph(args: &[String]) -> Result<ExitCode, String> {
    let path = positional(args, "<bundle>")?;
    let dot = has_flag(args, "--dot");
    let bundle = load(path)?;

    if dot {
        println!("digraph okf {{");
        println!("  rankdir=LR; node [shape=box, fontsize=10];");
        for c in bundle.concepts() {
            for link in bundle.links_from(&c.id) {
                let style = if link.exists { "" } else { " [style=dashed, color=red]" };
                println!("  {:?} -> {:?}{style};", c.id.to_string(), link.target.to_string());
            }
        }
        println!("}}");
    } else {
        for c in bundle.concepts() {
            let links = bundle.links_from(&c.id);
            if links.is_empty() {
                continue;
            }
            println!("{}", c.id);
            for link in links {
                let mark = if link.exists { "->" } else { "-x" };
                println!("  {mark} {}", link.target);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_parse(args: &[String]) -> Result<ExitCode, String> {
    let path = positional(args, "<file>")?;
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let doc = Document::parse(&text).map_err(|e| e.to_string())?;

    println!("frontmatter ({} key(s)):", doc.frontmatter.as_mapping().len());
    for (k, v) in doc.frontmatter.as_mapping().iter() {
        println!("  {k}: {v}");
    }
    let conformant = doc.validate_conformance().is_ok();
    println!("\nhas non-empty `type`: {conformant}");
    println!("body: {} byte(s)", doc.body.len());

    let links = doc.links();
    if !links.is_empty() {
        println!("\nlinks ({}):", links.len());
        for l in &links {
            println!("  [{:?}] {} -> {}", l.kind, l.text, l.target);
        }
    }
    let citations = doc.citations();
    if !citations.is_empty() {
        println!("\ncitations ({}):", citations.len());
        for cit in &citations {
            println!("  [{}] {}", cit.number, cit.raw);
        }
    }
    if conformant {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

fn cmd_fmt(args: &[String]) -> Result<ExitCode, String> {
    let path = positional(args, "<file>")?;
    let write = has_flag(args, "-w") || has_flag(args, "--write");
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let doc = Document::parse(&text).map_err(|e| e.to_string())?;
    let out = doc.serialize();

    if write {
        std::fs::write(Path::new(path), &out).map_err(|e| e.to_string())?;
        println!("formatted {path}");
    } else {
        print!("{out}");
    }
    Ok(ExitCode::SUCCESS)
}
