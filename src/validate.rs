//! Conformance checking against OKF v0.1 §9.
//!
//! A bundle is **conformant** if (1) every non-reserved `.md` file has a
//! parseable frontmatter block, (2) every frontmatter has a non-empty `type`,
//! and (3) reserved files follow their structure when present. Everything else
//! is soft guidance: consumers MUST NOT reject a bundle for missing optional
//! fields, unknown types/keys, broken links, or missing `index.md` files.
//!
//! Accordingly, [`validate_bundle`] reports only true §9 violations as
//! [`Severity::Error`]; all softer issues are [`Severity::Warning`] or
//! [`Severity::Info`].

use crate::bundle::Bundle;
use crate::concept_id::ConceptId;
use crate::document::Document;
use crate::log::{is_iso_date, Log};
use std::fs;
use std::path::PathBuf;

/// Severity of a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// A §9 conformance violation.
    Error,
    /// A soft-guidance deviation (the bundle is still conformant).
    Warning,
    /// Informational note (e.g. a broken but permitted cross-link).
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        })
    }
}

/// A single finding about a bundle.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// How serious the finding is.
    pub severity: Severity,
    /// The file the finding relates to, if any.
    pub path: Option<PathBuf>,
    /// The concept the finding relates to, if any.
    pub concept: Option<ConceptId>,
    /// A human-readable message.
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] ", self.severity)?;
        if let Some(p) = &self.path {
            write!(f, "{}: ", p.display())?;
        } else if let Some(c) = &self.concept {
            write!(f, "{c}: ")?;
        }
        f.write_str(&self.message)
    }
}

/// The result of validating a bundle.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// All findings, errors first by construction order.
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    /// `true` if there are no [`Severity::Error`] diagnostics — i.e. the bundle
    /// conforms to §9.
    pub fn is_conformant(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    /// Iterates over diagnostics of a given severity.
    pub fn of(&self, severity: Severity) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.severity == severity)
    }

    /// Count of error-level diagnostics.
    pub fn error_count(&self) -> usize {
        self.of(Severity::Error).count()
    }

    /// Count of warning-level diagnostics.
    pub fn warning_count(&self) -> usize {
        self.of(Severity::Warning).count()
    }
}

/// Validates a loaded bundle against §9, returning all findings.
pub fn validate_bundle(bundle: &Bundle) -> Report {
    let mut report = Report::default();

    // (1) Files whose frontmatter could not be parsed are conformance errors.
    for (path, error) in bundle.parse_errors() {
        report.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            path: Some(path.clone()),
            concept: None,
            message: format!("unparseable concept document: {error}"),
        });
    }

    // (2) Every concept must carry a non-empty `type`; recommended fields are
    // soft guidance.
    for concept in bundle.concepts() {
        let fm = &concept.document.frontmatter;
        if concept.document.validate_conformance().is_err() {
            report.diagnostics.push(Diagnostic {
                severity: Severity::Error,
                path: Some(concept.path.clone()),
                concept: Some(concept.id.clone()),
                message: "missing required frontmatter field `type`".to_string(),
            });
        }
        for field in ["title", "description", "timestamp"] {
            if fm.get(field).map(|v| v.is_empty_value()).unwrap_or(true) {
                report.diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    path: Some(concept.path.clone()),
                    concept: Some(concept.id.clone()),
                    message: format!("missing recommended frontmatter field `{field}`"),
                });
            }
        }
        if let Some(ts) = fm.timestamp() {
            if !is_iso8601_datetime(&ts) {
                report.diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    path: Some(concept.path.clone()),
                    concept: Some(concept.id.clone()),
                    message: format!("`timestamp` is not ISO-8601: {ts:?}"),
                });
            }
        }
    }

    // (3) Reserved files must follow their structure when present.
    validate_reserved(bundle, &mut report);

    // Broken cross-links are permitted (§5.3); report them as info only.
    for (source, raw) in bundle.broken_links() {
        report.diagnostics.push(Diagnostic {
            severity: Severity::Info,
            path: None,
            concept: Some(source),
            message: format!("link target does not resolve to a concept in the bundle: {raw}"),
        });
    }

    report
}

fn validate_reserved(bundle: &Bundle, report: &mut Report) {
    let root_index = bundle.root().join("index.md");

    for path in bundle.index_files() {
        let Ok(text) = fs::read_to_string(path) else { continue };
        let Ok(doc) = Document::parse(&text) else { continue };
        if doc.frontmatter.is_empty() {
            continue;
        }
        // Frontmatter is only permitted in the bundle-root index.md, and only
        // to declare `okf_version` (§11).
        let is_root = path == &root_index;
        if !is_root {
            report.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                path: Some(path.clone()),
                concept: None,
                message: "index.md should not contain frontmatter (§6)".to_string(),
            });
        } else {
            let only_version = doc.frontmatter.as_mapping().keys().all(|k| k == "okf_version");
            if !only_version {
                report.diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    path: Some(path.clone()),
                    concept: None,
                    message: "root index.md frontmatter should declare only `okf_version` (§11)"
                        .to_string(),
                });
            }
        }
    }

    for path in bundle.log_files() {
        let Ok(text) = fs::read_to_string(path) else { continue };
        let log = Log::parse(&text);
        for bad in log.invalid_dates() {
            report.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                path: Some(path.clone()),
                concept: None,
                message: format!("log date heading is not ISO-8601 `YYYY-MM-DD`: {bad:?}"),
            });
        }
    }
}

/// Light ISO-8601 datetime check: a valid `YYYY-MM-DD` date, optionally followed
/// by `T<time>` with an optional zone. This is intentionally permissive — the
/// spec treats `timestamp` formatting as soft guidance.
pub fn is_iso8601_datetime(s: &str) -> bool {
    let date_part = s.split(['T', ' ']).next().unwrap_or(s);
    is_iso_date(date_part)
}
