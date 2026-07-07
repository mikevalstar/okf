//! Bundle loading, the cross-link graph, and conformance, exercised against the
//! spec's Appendix A minimal example bundle.

mod common;

use common::TempDir;
use okf::{validate_bundle, Bundle, ConceptId, Severity};

/// Builds the Appendix A example bundle and returns its temp dir.
fn appendix_a() -> TempDir {
    let tmp = TempDir::new();
    tmp.write(
        "datasets/sales.md",
        "---\n\
         type: BigQuery Dataset\n\
         title: Sales\n\
         description: All sales-related tables for the retail business.\n\
         resource: https://console.cloud.google.com/bigquery?p=acme&d=sales\n\
         tags: [sales]\n\
         timestamp: 2026-05-28T00:00:00Z\n\
         ---\n\n\
         The sales dataset contains transactional tables, including\n\
         [orders](/tables/orders.md) and [customers](/tables/customers.md).\n",
    );
    tmp.write(
        "tables/orders.md",
        "---\n\
         type: BigQuery Table\n\
         title: Orders\n\
         description: One row per completed customer order.\n\
         resource: https://console.cloud.google.com/bigquery?p=acme&d=sales&t=orders\n\
         tags: [sales, orders]\n\
         timestamp: 2026-05-28T00:00:00Z\n\
         ---\n\n\
         # Schema\n\n\
         Part of the [sales dataset](/datasets/sales.md). FK to [customers](/tables/customers.md).\n",
    );
    tmp.write(
        "tables/customers.md",
        "---\n\
         type: BigQuery Table\n\
         title: Customers\n\
         description: One row per customer.\n\
         timestamp: 2026-05-28T00:00:00Z\n\
         ---\n\n\
         Linked from [orders](/tables/orders.md).\n",
    );
    tmp
}

#[test]
fn loads_all_concepts() {
    let tmp = appendix_a();
    let bundle = Bundle::load(tmp.path()).unwrap();
    assert_eq!(bundle.len(), 3);
    assert!(bundle.contains(&ConceptId::parse("tables/orders").unwrap()));
    assert!(bundle.contains(&ConceptId::parse("datasets/sales").unwrap()));
    assert!(bundle.parse_errors().is_empty());
}

#[test]
fn loads_and_links_concepts_with_spaces_in_filenames() {
    let tmp = TempDir::new();
    tmp.write(
        "reports/Quarterly Report.md",
        "---\n\
         type: Report\n\
         title: Quarterly Report\n\
         ---\n\n\
         Builds on the [annual summary](/reports/Annual%20Summary.md).\n",
    );
    tmp.write(
        "reports/Annual Summary.md",
        "---\n\
         type: Report\n\
         title: Annual Summary\n\
         ---\n\n\
         Rolls up each [quarterly report](./Quarterly Report.md).\n",
    );

    let bundle = Bundle::load(tmp.path()).unwrap();
    assert_eq!(bundle.len(), 2);
    assert!(bundle.parse_errors().is_empty());

    let quarterly = ConceptId::parse("reports/Quarterly Report").unwrap();
    let annual = ConceptId::parse("reports/Annual Summary").unwrap();
    assert!(bundle.contains(&quarterly));
    assert!(bundle.contains(&annual));

    // A percent-encoded link resolves to the space-containing concept...
    let from_q: Vec<_> = bundle.links_from(&quarterly).iter().map(|l| l.target.clone()).collect();
    assert!(from_q.contains(&annual));
    assert!(bundle.links_from(&quarterly).iter().all(|l| l.exists));

    // ...and a raw-space relative link resolves too, producing a backlink.
    let from_a: Vec<_> = bundle.links_from(&annual).iter().map(|l| l.target.clone()).collect();
    assert!(from_a.contains(&quarterly));
    assert!(bundle.backlinks(&quarterly).contains(&annual));
    assert!(bundle.broken_links().is_empty());
}

#[test]
fn resolves_cross_links_and_backlinks() {
    let tmp = appendix_a();
    let bundle = Bundle::load(tmp.path()).unwrap();

    let sales = ConceptId::parse("datasets/sales").unwrap();
    let orders = ConceptId::parse("tables/orders").unwrap();
    let customers = ConceptId::parse("tables/customers").unwrap();

    let sales_links: Vec<_> = bundle.links_from(&sales).iter().map(|l| l.target.clone()).collect();
    assert!(sales_links.contains(&orders));
    assert!(sales_links.contains(&customers));
    assert!(bundle.links_from(&sales).iter().all(|l| l.exists));

    // orders is linked from sales and customers.
    let backlinks = bundle.backlinks(&orders);
    assert!(backlinks.contains(&sales));
    assert!(backlinks.contains(&customers));

    assert!(bundle.broken_links().is_empty());
}

#[test]
fn broken_links_are_detected_but_not_fatal() {
    let tmp = TempDir::new();
    tmp.write(
        "a.md",
        "---\ntype: Note\n---\nSee [missing](/does/not/exist.md).\n",
    );
    let bundle = Bundle::load(tmp.path()).unwrap();
    let broken = bundle.broken_links();
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0].1, "/does/not/exist.md");

    // Broken links are informational, not conformance errors.
    let report = validate_bundle(&bundle);
    assert!(report.is_conformant());
    assert!(report.of(Severity::Info).any(|d| d.message.contains("does/not/exist")));
}

#[test]
fn appendix_a_is_conformant() {
    let tmp = appendix_a();
    let bundle = Bundle::load(tmp.path()).unwrap();
    let report = validate_bundle(&bundle);
    assert!(report.is_conformant(), "{:#?}", report.diagnostics);
    assert_eq!(report.error_count(), 0);
}

#[test]
fn missing_type_is_a_conformance_error() {
    let tmp = TempDir::new();
    tmp.write("bad.md", "---\ntitle: No Type\n---\nbody\n");
    let bundle = Bundle::load(tmp.path()).unwrap();
    let report = validate_bundle(&bundle);
    assert!(!report.is_conformant());
    assert!(report.of(Severity::Error).any(|d| d.message.contains("type")));
}

#[test]
fn reserved_files_are_recognized_not_concepts() {
    let tmp = TempDir::new();
    tmp.write("a.md", "---\ntype: Note\n---\nbody\n");
    tmp.write("index.md", "# Listing\n\n* [a](a.md)\n");
    tmp.write("log.md", "# Log\n\n## 2026-05-22\n* **Update**: did a thing.\n");
    let bundle = Bundle::load(tmp.path()).unwrap();
    assert_eq!(bundle.len(), 1); // only a.md is a concept
    assert_eq!(bundle.index_files().len(), 1);
    assert_eq!(bundle.log_files().len(), 1);
}

#[test]
fn okf_version_read_from_root_index() {
    let tmp = TempDir::new();
    tmp.write("a.md", "---\ntype: Note\n---\nbody\n");
    tmp.write("index.md", "---\nokf_version: \"0.1\"\n---\n\n# Listing\n");
    let bundle = Bundle::load(tmp.path()).unwrap();
    assert_eq!(bundle.okf_version().as_deref(), Some("0.1"));
}
