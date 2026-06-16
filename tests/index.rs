//! Index generation tests, mirroring the reference `tests/test_index.py`.

mod common;

use common::TempDir;
use okf::index::{regenerate_indexes, regenerate_indexes_with};

fn write_doc(tmp: &TempDir, rel: &str, type_: &str, title: &str, description: &str) {
    let contents = format!(
        "---\ntype: {type_}\ntitle: {title}\ndescription: {description}\ntimestamp: 2026-05-27T00:00:00+00:00\n---\n\n# {title}\n\n{description}\n"
    );
    tmp.write(rel, &contents);
}

#[test]
fn regenerate_groups_by_type_and_links_relative() {
    let tmp = TempDir::new();
    write_doc(&tmp, "datasets/ga4.md", "BigQuery Dataset", "GA4 Dataset", "GA4 obfuscated ecommerce sample.");
    write_doc(&tmp, "tables/events_.md", "BigQuery Table", "events_*", "Daily-sharded GA4 event tables.");
    write_doc(&tmp, "tables/users.md", "BigQuery Table", "users", "Per-user dimension.");

    // Deterministic synthesizer so we can assert on the root index text.
    let synth = |_rel: &str, children: &[(String, String)]| format!("stub: {} items", children.len());
    let written = regenerate_indexes_with(tmp.path(), &synth).unwrap();
    assert!(!written.is_empty());

    let tables_index = tmp.read("tables/index.md");
    assert!(tables_index.starts_with("# BigQuery Table"), "{tables_index}");
    assert!(tables_index.contains("[events_*](events_.md)"), "{tables_index}");
    assert!(tables_index.contains("[users](users.md)"), "{tables_index}");
    assert!(tables_index.contains("Daily-sharded GA4 event tables."));

    let root_index = tmp.read("index.md");
    assert!(root_index.contains("# Subdirectories"), "{root_index}");
    assert!(root_index.contains("(datasets/index.md) - GA4 obfuscated ecommerce sample."), "{root_index}");
    assert!(root_index.contains("(tables/index.md) - stub: 2 items"), "{root_index}");
}

#[test]
fn regenerate_skips_empty_directories() {
    let tmp = TempDir::new();
    tmp.mkdir("empty_dir");
    let written = regenerate_indexes(tmp.path()).unwrap();
    assert!(written.is_empty());
    assert!(!tmp.path().join("empty_dir/index.md").exists());
}

#[test]
fn regenerate_single_child_reuses_description() {
    let tmp = TempDir::new();
    write_doc(&tmp, "datasets/only.md", "BigQuery Dataset", "Only Dataset", "The only dataset in this bundle.");

    let calls = std::cell::Cell::new(0u32);
    let counting = |_rel: &str, children: &[(String, String)]| {
        calls.set(calls.get() + 1);
        format!("stub: {} items", children.len())
    };
    regenerate_indexes_with(tmp.path(), &counting).unwrap();

    let root_index = tmp.read("index.md");
    assert!(
        root_index.contains("(datasets/index.md) - The only dataset in this bundle."),
        "{root_index}"
    );
    assert_eq!(calls.get(), 0, "single child with a description should be reused, not synthesized");
}
