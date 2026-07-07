//! Concept-id parsing and segment validation (§2), including the fork's
//! support for spaces in file names.

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
fn rejects_leading_or_trailing_spaces() {
    assert!(validate_segment(" leading").is_err());
    assert!(validate_segment("trailing ").is_err());
    assert!(validate_segment(" ").is_err());
    // A space still cannot be the first character of a segment.
    assert!(ConceptId::parse("a/ b").is_err());
}

#[test]
fn still_rejects_other_disallowed_characters() {
    assert!(validate_segment("bad?name").is_err());
    assert!(validate_segment("percent%20encoded").is_err());
    assert!(validate_segment("slash/inside").is_err());
}

#[test]
fn path_round_trip_preserves_spaces() {
    let root = Path::new("/bundle");
    let id = ConceptId::parse("reports/Quarterly Report").unwrap();
    let path = id.to_path(root);
    assert_eq!(path, Path::new("/bundle/reports/Quarterly Report.md"));

    let back = ConceptId::from_path(root, &path).unwrap();
    assert_eq!(back, id);
}
