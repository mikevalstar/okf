//! Concept-id parsing and segment validation (§2), including the fork's
//! widened rule permitting spaces, emoji, and other Unicode in file names.

use okf::concept_id::validate_segment;
use okf::ConceptId;
use std::path::Path;

#[test]
fn accepts_spaces_in_segments() {
    validate_segment("Quarterly Report").unwrap();
    validate_segment("my file name").unwrap();

    let id = ConceptId::parse("reports/Quarterly Report").unwrap();
    assert_eq!(id.segments(), &["reports", "Quarterly Report"]);
    assert_eq!(id.name(), "Quarterly Report");
    // Display round-trips the space unchanged.
    assert_eq!(id.to_string(), "reports/Quarterly Report");
}

#[test]
fn accepts_emoji_and_unicode_segments() {
    // Interior emoji, and an emoji as the very first character.
    validate_segment("Q1 🚀 Launch").unwrap();
    validate_segment("🚀 Launch").unwrap();
    validate_segment("🚀").unwrap();
    // Accented Latin and CJK.
    validate_segment("café").unwrap();
    validate_segment("设计").unwrap();

    let id = ConceptId::parse("reports/Q1 🚀 Launch").unwrap();
    assert_eq!(id.name(), "Q1 🚀 Launch");
    assert_eq!(id.to_string(), "reports/Q1 🚀 Launch");
}

#[test]
fn rejects_leading_or_trailing_spaces() {
    assert!(validate_segment(" leading").is_err());
    assert!(validate_segment("trailing ").is_err());
    assert!(validate_segment(" ").is_err());
    // A space still cannot be the first character of a segment.
    assert!(ConceptId::parse("a/ b").is_err());
}

#[test]
fn rejects_leading_dot_or_dash() {
    // Hidden-file and option-like names stay out.
    assert!(validate_segment(".hidden").is_err());
    assert!(validate_segment("-flag").is_err());
    assert!(validate_segment("..").is_err());
    // But `.`/`-` are fine once they're interior.
    validate_segment("foo.bar").unwrap();
    validate_segment("foo-bar").unwrap();
    validate_segment("0001-adr").unwrap();
}

#[test]
fn rejects_path_hostile_characters() {
    // Path separators and the Windows-reserved set are always rejected.
    for bad in [
        "slash/inside",
        "back\\slash",
        "colon:name",
        "star*name",
        "question?name",
        "quote\"name",
        "angle<name",
        "angle>name",
        "pipe|name",
    ] {
        assert!(
            validate_segment(bad).is_err(),
            "expected {bad:?} to be rejected"
        );
    }
    // Control characters (tab, newline, NUL) are rejected.
    assert!(validate_segment("tab\tname").is_err());
    assert!(validate_segment("newline\nname").is_err());
    assert!(validate_segment("nul\0name").is_err());
}

#[test]
fn allows_characters_the_narrow_rule_rejected() {
    // The widening is a denylist: a literal `%` or parens in a filename are fine
    // now (they are neither separators nor reserved), where the ASCII rule
    // rejected them.
    validate_segment("percent%20literal").unwrap();
    validate_segment("notes (draft)").unwrap();
    validate_segment("a+b=c").unwrap();
}

#[test]
fn path_round_trip_preserves_spaces_and_emoji() {
    let root = Path::new("/bundle");
    for id_str in ["reports/Quarterly Report", "reports/Q1 🚀 Launch", "café"] {
        let id = ConceptId::parse(id_str).unwrap();
        let path = id.to_path(root);
        let back = ConceptId::from_path(root, &path).unwrap();
        assert_eq!(back, id, "round-trip failed for {id_str:?}");
    }
}
