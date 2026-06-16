//! Document parsing/serialization/validation tests.
//!
//! These mirror the reference implementation's `tests/test_document.py` to
//! guarantee behavioural parity, plus extra edge cases.

use okf::yaml::Value;
use okf::{Document, DocumentError};

#[test]
fn roundtrip_preserves_frontmatter_and_body() {
    let src = "---\n\
        type: BigQuery Table\n\
        title: Sample\n\
        description: A sample table.\n\
        tags: [a, b]\n\
        timestamp: 2026-05-27T00:00:00+00:00\n\
        ---\n\
        \n\
        # Sample\n\
        \n\
        Body text.\n";
    let doc = Document::parse(src).unwrap();
    assert_eq!(doc.frontmatter.type_().as_deref(), Some("BigQuery Table"));
    assert_eq!(doc.frontmatter.tags(), vec!["a", "b"]);
    assert!(doc.body.starts_with("# Sample"));

    let serialized = doc.serialize();
    let reparsed = Document::parse(&serialized).unwrap();
    assert_eq!(reparsed.frontmatter, doc.frontmatter);
    assert_eq!(reparsed.body.trim(), doc.body.trim());
}

#[test]
fn parse_no_frontmatter_treats_all_as_body() {
    let src = "# Hello\n\nNo frontmatter here.\n";
    let doc = Document::parse(src).unwrap();
    assert!(doc.frontmatter.is_empty());
    assert!(doc.body.contains("Hello"));
}

#[test]
fn unterminated_frontmatter_raises() {
    let src = "---\ntype: X\nstill in frontmatter\n";
    let err = Document::parse(src).unwrap_err();
    assert_eq!(err, DocumentError::UnterminatedFrontmatter);
}

#[test]
fn validate_rejects_missing_required_keys() {
    let doc = Document::parse("---\ntype: X\ntitle: Y\n---\n").unwrap();
    let err = doc.validate().unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("description"), "{msg}");
    assert!(msg.contains("timestamp"), "{msg}");
}

#[test]
fn validate_accepts_full_frontmatter() {
    let doc = Document::parse(
        "---\ntype: X\ntitle: Y\ndescription: Z\ntimestamp: 2026-05-27T00:00:00+00:00\n---\n",
    )
    .unwrap();
    assert!(doc.validate().is_ok());
}

#[test]
fn conformance_requires_only_type() {
    let doc = Document::parse("---\ntype: Metric\n---\nbody\n").unwrap();
    assert!(doc.validate_conformance().is_ok());
    assert!(doc.validate().is_err()); // strict producer validation still fails

    let no_type = Document::parse("---\ntitle: X\n---\n").unwrap();
    assert!(no_type.validate_conformance().is_err());
}

#[test]
fn empty_type_is_not_conformant() {
    let doc = Document::parse("---\ntype: \"\"\n---\n").unwrap();
    assert!(doc.validate_conformance().is_err());
}

#[test]
fn unknown_keys_are_preserved_on_roundtrip() {
    let src = "---\ntype: X\ncustom_key: custom value\nnested:\n  a: 1\n  b: 2\n---\nbody\n";
    let doc = Document::parse(src).unwrap();
    assert!(doc.frontmatter.get("custom_key").is_some());
    let extensions = doc.frontmatter.extension_keys();
    assert!(extensions.contains(&"custom_key"));
    assert!(extensions.contains(&"nested"));

    let reparsed = Document::parse(&doc.serialize()).unwrap();
    assert_eq!(reparsed.frontmatter, doc.frontmatter);
    assert_eq!(
        reparsed.frontmatter.get("nested"),
        Some(&Value::parse("{a: 1, b: 2}").unwrap())
    );
}

#[test]
fn empty_frontmatter_block_is_empty_mapping() {
    let doc = Document::parse("---\n---\nbody\n").unwrap();
    assert!(doc.frontmatter.is_empty());
    // The trailing newline is dropped on parse (matching the reference's
    // splitlines/join); serialize restores it.
    assert_eq!(doc.body, "body");
    assert!(doc.serialize().ends_with("body\n"));
}
