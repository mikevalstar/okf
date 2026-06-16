//! YAML-subset parser/emitter tests, including the round-trip invariant.

use okf::yaml::Value;

fn roundtrip(src: &str) -> Value {
    let v = Value::parse(src).unwrap();
    let emitted = v.to_yaml_string();
    let reparsed = Value::parse(&emitted).unwrap();
    assert_eq!(v, reparsed, "round-trip mismatch.\nsrc:\n{src}\nemitted:\n{emitted}");
    v
}

#[test]
fn scalars() {
    assert_eq!(Value::parse("hello").unwrap(), Value::String("hello".into()));
    assert_eq!(Value::parse("42").unwrap(), Value::Int(42));
    assert_eq!(Value::parse("-7").unwrap(), Value::Int(-7));
    assert_eq!(Value::parse("2.5").unwrap(), Value::Float(2.5));
    assert_eq!(Value::parse("true").unwrap(), Value::Bool(true));
    assert_eq!(Value::parse("false").unwrap(), Value::Bool(false));
    assert_eq!(Value::parse("null").unwrap(), Value::Null);
    assert_eq!(Value::parse("~").unwrap(), Value::Null);
    assert_eq!(Value::parse("").unwrap(), Value::Null);
}

#[test]
fn quoted_scalars() {
    assert_eq!(Value::parse("\"42\"").unwrap(), Value::String("42".into()));
    assert_eq!(Value::parse("'true'").unwrap(), Value::String("true".into()));
    assert_eq!(
        Value::parse("\"line1\\nline2\"").unwrap(),
        Value::String("line1\nline2".into())
    );
    assert_eq!(
        Value::parse("'it''s here'").unwrap(),
        Value::String("it's here".into())
    );
}

#[test]
fn block_mapping() {
    let v = roundtrip("type: BigQuery Table\ntitle: Orders\ncount: 3\n");
    let m = v.as_mapping().unwrap();
    assert_eq!(m.get("type").unwrap().as_str(), Some("BigQuery Table"));
    assert_eq!(m.get("count").unwrap().as_int(), Some(3));
    // Key order is preserved.
    assert_eq!(m.keys().collect::<Vec<_>>(), vec!["type", "title", "count"]);
}

#[test]
fn flow_and_block_sequences() {
    let flow = roundtrip("tags: [sales, orders, revenue]\n");
    assert_eq!(
        flow.as_mapping().unwrap().get("tags").unwrap().as_sequence().unwrap().len(),
        3
    );
    let block = roundtrip("tags:\n  - sales\n  - orders\n");
    let tags = block.as_mapping().unwrap().get("tags").unwrap();
    assert_eq!(tags.as_sequence().unwrap()[0].as_str(), Some("sales"));
}

#[test]
fn nested_mappings() {
    roundtrip("a:\n  b:\n    c: deep\n  d: 2\ne: top\n");
}

#[test]
fn flow_mapping() {
    let v = roundtrip("obj: {x: 1, y: two}\n");
    let obj = v.as_mapping().unwrap().get("obj").unwrap().as_mapping().unwrap();
    assert_eq!(obj.get("x").unwrap().as_int(), Some(1));
    assert_eq!(obj.get("y").unwrap().as_str(), Some("two"));
}

#[test]
fn comments_are_ignored() {
    let v = Value::parse("# leading comment\ntype: X  # trailing\ntitle: Y\n").unwrap();
    let m = v.as_mapping().unwrap();
    assert_eq!(m.get("type").unwrap().as_str(), Some("X"));
    assert_eq!(m.get("title").unwrap().as_str(), Some("Y"));
}

#[test]
fn literal_block_scalar() {
    let v = Value::parse("body: |\n  line one\n  line two\n").unwrap();
    assert_eq!(
        v.as_mapping().unwrap().get("body").unwrap().as_str(),
        Some("line one\nline two\n")
    );
}

#[test]
fn folded_block_scalar() {
    let v = Value::parse("body: >\n  line one\n  line two\n").unwrap();
    assert_eq!(
        v.as_mapping().unwrap().get("body").unwrap().as_str(),
        Some("line one line two\n")
    );
}

#[test]
fn strings_needing_quotes_roundtrip() {
    // A string that looks like a number / bool / has special chars must be
    // quoted on emit so it re-parses as a string.
    for s in ["42", "true", "null", "a: b", "value # x", "", "  spaced  "] {
        let v = Value::String(s.to_string());
        let emitted = Value::Mapping({
            let mut m = okf::yaml::Mapping::new();
            m.insert("k", v.clone());
            m
        })
        .to_yaml_string();
        let reparsed = Value::parse(&emitted).unwrap();
        assert_eq!(
            reparsed.as_mapping().unwrap().get("k"),
            Some(&v),
            "string {s:?} did not round-trip; emitted: {emitted}"
        );
    }
}

#[test]
fn block_sequence_at_parent_indent() {
    // This is exactly what PyYAML's safe_dump (the reference serializer) emits
    // for list values: dashes at the same column as the key.
    let v = Value::parse("type: X\ntags:\n- sales\n- orders\ntitle: Y\n").unwrap();
    let m = v.as_mapping().unwrap();
    let tags = m.get("tags").unwrap().as_sequence().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].as_str(), Some("sales"));
    assert_eq!(m.get("title").unwrap().as_str(), Some("Y"));
    // And nested under a deeper mapping.
    let nested = Value::parse("outer:\n  tags:\n  - a\n  - b\n").unwrap();
    let inner = nested.as_mapping().unwrap().get("outer").unwrap().as_mapping().unwrap();
    assert_eq!(inner.get("tags").unwrap().as_sequence().unwrap().len(), 2);
}

#[test]
fn conservative_number_resolution() {
    // Zero-padded codes stay strings (not coerced to ints).
    assert_eq!(Value::parse("007").unwrap(), Value::String("007".into()));
    assert_eq!(Value::parse("08").unwrap(), Value::String("08".into()));
    // Bare-exponent forms stay strings; only point-bearing floats are floats.
    assert_eq!(Value::parse("1e3").unwrap(), Value::String("1e3".into()));
    assert_eq!(Value::parse("1.5e3").unwrap(), Value::Float(1500.0));
    assert_eq!(Value::parse("0").unwrap(), Value::Int(0));
    assert_eq!(Value::parse("-42").unwrap(), Value::Int(-42));
}

#[test]
fn non_finite_and_large_floats_roundtrip() {
    for f in [f64::INFINITY, f64::NEG_INFINITY, 1e30, -2.5e-12, 1.0] {
        let v = Value::Float(f);
        let mut m = okf::yaml::Mapping::new();
        m.insert("k", v.clone());
        let emitted = Value::Mapping(m).to_yaml_string();
        let reparsed = Value::parse(&emitted).unwrap();
        let got = reparsed.as_mapping().unwrap().get("k").unwrap();
        match got {
            Value::Float(g) => assert_eq!(g.to_bits(), f.to_bits(), "emitted: {emitted}"),
            other => panic!("{f} round-tripped as {other:?} (emitted: {emitted})"),
        }
    }
    // NaN is a float on the way back (compared specially).
    let mut m = okf::yaml::Mapping::new();
    m.insert("k", Value::Float(f64::NAN));
    let reparsed = Value::parse(&Value::Mapping(m).to_yaml_string()).unwrap();
    assert!(matches!(reparsed.as_mapping().unwrap().get("k"), Some(Value::Float(g)) if g.is_nan()));
}

#[test]
fn unterminated_flow_is_error() {
    assert!(Value::parse("tags: [a, b").is_err());
}

#[test]
fn tab_indentation_is_error() {
    assert!(Value::parse("a:\n\tb: 1").is_err());
}
