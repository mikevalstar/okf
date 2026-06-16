# okf

A **pure-Rust, zero-dependency** implementation of the [Open Knowledge Format
(OKF) v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) —
Google's open, human- and agent-friendly format for representing *knowledge* as
a directory of markdown files with YAML frontmatter.

> OKF is intentionally minimal: "if you can `cat` a file, you can read OKF; if
> you can `git clone` a repo, you can ship it." This crate honors that spirit —
> it is implemented entirely on the Rust **standard library**, with **no
> third-party dependencies** (it includes its own YAML-subset parser, markdown
> link scanner, directory walker, and CLI argument parsing).

## What OKF is

- A **bundle** is a directory tree of UTF-8 markdown files (the unit of
  distribution).
- A **concept** is one markdown document: a YAML **frontmatter** block delimited
  by `---`, followed by a markdown **body**.
- A **concept id** is the file's path within the bundle with `.md` removed
  (`tables/users.md` → `tables/users`).
- Concepts **cross-link** via ordinary markdown links — absolute
  (`/tables/users.md`, bundle-relative) or relative (`./other.md`).
- `index.md` files provide directory listings for *progressive disclosure*;
  `log.md` files record date-grouped change history. Both are **reserved**
  filenames.
- The only hard requirement for **conformance** is a non-empty `type` field on
  every concept; consumers must otherwise be permissive (unknown types, unknown
  keys, broken links, and missing optional fields are all tolerated).

See [`SPEC.md` summary](#mapping-to-the-spec) below for the section-by-section
mapping.

## Library overview

| Module          | Responsibility                                                        |
|-----------------|-----------------------------------------------------------------------|
| [`yaml`]        | A YAML-*subset* `Value`/`Mapping`, parser, and emitter for frontmatter |
| [`document`]    | `Document` = frontmatter + body; parse / serialize / validate (§4)    |
| [`frontmatter`] | `Frontmatter`: typed accessors over an order-preserving mapping (§4.1)|
| [`concept_id`]  | `ConceptId` ↔ path conversion and segment validation (§2)             |
| [`links`]       | Markdown link extraction, classification, resolution, citations (§5, §8) |
| [`bundle`]      | `Bundle::load` — walk a tree, build the concept graph + backlinks (§3, §5) |
| [`index`]       | Generate `index.md` directory listings (§6)                           |
| [`log`]         | Parse / build `log.md` update histories (§7)                          |
| [`validate`]    | §9 conformance checking with severity-tagged diagnostics              |

The split mirrors the reference Python implementation's `bundle/` package
(`document.py`, `index.py`, `paths.py`) so behaviour stays compatible — the
document parser, validator, and index generator are faithful ports, verified by
tests adapted from the reference test suite.

### Design choices

- **Frontmatter preserves everything.** Rather than deserializing into a fixed
  struct (which would drop producer-defined keys), `Frontmatter` keeps the full
  ordered mapping and layers typed getters (`type_()`, `title()`, `tags()`, …)
  on top. This satisfies the spec's requirement that consumers preserve unknown
  keys when round-tripping.
- **Permissive loading.** `Bundle::load` never aborts on a bad concept file; it
  collects parse failures in `parse_errors()` and keeps going. Broken
  cross-links are retained as graph edges to non-existent concepts.
- **Two levels of validation.** `Document::validate_conformance()` enforces only
  what §9 requires (a non-empty `type`). `Document::validate()` matches the
  stricter producer-side check from the reference agent (`type`, `title`,
  `description`, `timestamp`).
- **A documented YAML subset.** Real OKF frontmatter is scalars, lists, and
  shallow maps. The parser handles block/flow collections, quoted/plain
  scalars, `|`/`>` block scalars, and comments; it rejects (with a clear error)
  the YAML features that never appear in frontmatter — anchors, tags, multiple
  documents.

## Usage

### As a library

```rust
use okf::{Bundle, validate_bundle, ConceptId};

let bundle = Bundle::load("./my_bundle")?;
println!("{} concepts", bundle.len());

// Conformance check (§9).
let report = validate_bundle(&bundle);
if report.is_conformant() {
    println!("conformant with OKF v{}", okf::OKF_VERSION);
}

// Traverse the cross-link graph.
let id = ConceptId::parse("tables/orders")?;
for link in bundle.links_from(&id) {
    println!("{} -> {} (exists: {})", id, link.target, link.exists);
}
for backlink in bundle.backlinks(&id) {
    println!("cited by {backlink}");
}
# Ok::<(), okf::BundleError>(())
```

Parsing and round-tripping a single document:

```rust
use okf::Document;

let doc = Document::parse("---\ntype: Metric\ntitle: DAU\n---\n\n# Body\n")?;
assert_eq!(doc.frontmatter.type_().as_deref(), Some("Metric"));
assert!(doc.validate_conformance().is_ok());

// serialize() preserves frontmatter key order and the body.
let text = doc.serialize();
# Ok::<(), okf::DocumentError>(())
```

### As a CLI

```
okf validate <bundle>    Check a bundle against OKF v0.1 conformance (§9)
okf info     <bundle>    Summarize a bundle (concepts, types, links, version)
okf index    <bundle>    (Re)generate every index.md in the bundle
okf graph    <bundle>    Print the cross-link graph (--dot for Graphviz DOT)
okf parse    <file>      Parse one concept document and print its structure
okf fmt      <file>      Normalize a document by parse + re-serialize (-w writes)
```

`okf validate` exits non-zero when a bundle is not conformant, so it drops
straight into CI:

```sh
okf validate ./bundles/ga4
okf graph ./bundles/ga4 --dot | dot -Tsvg > graph.svg
```

## Mapping to the spec

| Spec section                | Implemented by                                            |
|-----------------------------|-----------------------------------------------------------|
| §2 Terminology / concept id | [`concept_id::ConceptId`]                                 |
| §3 Bundle structure         | [`bundle::Bundle`], [`bundle::RESERVED_FILENAMES`]        |
| §4 Concept documents        | [`document::Document`], [`frontmatter::Frontmatter`]      |
| §5 Cross-linking            | [`links`], [`bundle::Bundle::links_from`] / `backlinks`   |
| §6 Index files              | [`index::regenerate_indexes`]                             |
| §7 Log files                | [`log::Log`]                                              |
| §8 Citations                | [`links::extract_citations`], [`document::Document::citations`] |
| §9 Conformance              | [`validate::validate_bundle`]                             |
| §11 Versioning              | [`bundle::Bundle::okf_version`], [`OKF_VERSION`]          |

## Building & testing

```sh
cargo build            # library + `okf` binary
cargo test             # unit + integration tests (incl. ports of the reference tests)
cargo clippy --all-targets
```

## License

Licensed under the **Apache License, Version 2.0** — the same license as the
upstream [OKF project](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf).
This crate is a derivative work: its document parser, concept-id conventions,
and index generator are ports of the OKF reference implementation. See
[`LICENSE`](LICENSE) for the full terms and [`NOTICE`](NOTICE) for attribution.

This is an independent implementation and is not affiliated with or endorsed by
Google.

[`yaml`]: https://docs.rs/okf/latest/okf/yaml/
[`document`]: https://docs.rs/okf/latest/okf/document/
[`frontmatter`]: https://docs.rs/okf/latest/okf/frontmatter/
[`concept_id`]: https://docs.rs/okf/latest/okf/concept_id/
[`links`]: https://docs.rs/okf/latest/okf/links/
[`bundle`]: https://docs.rs/okf/latest/okf/bundle/
[`index`]: https://docs.rs/okf/latest/okf/index/
[`log`]: https://docs.rs/okf/latest/okf/log/
[`validate`]: https://docs.rs/okf/latest/okf/validate/
