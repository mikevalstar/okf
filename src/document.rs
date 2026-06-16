//! The OKF concept document: YAML frontmatter + markdown body.
//!
//! This is a faithful port of the reference implementation's `OKFDocument`
//! (`okf/src/enrichment_agent/bundle/document.py`), including its exact parse,
//! serialize, and validation behaviour, so that documents round-trip
//! compatibly between the two implementations. Ported to Rust and modified from
//! the original Apache-2.0 Python source; see the NOTICE file.

use crate::error::DocumentError;
use crate::frontmatter::{Frontmatter, REQUIRED_FRONTMATTER_KEYS};
use crate::links::{self, Citation, Link};
use crate::yaml::Value;

const FRONTMATTER_DELIM: &str = "---";

/// A parsed OKF concept document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    /// The YAML frontmatter block (empty if the file had none).
    pub frontmatter: Frontmatter,
    /// Everything after the frontmatter.
    pub body: String,
}

impl Document {
    /// Creates a document from frontmatter and a body.
    pub fn new(frontmatter: Frontmatter, body: impl Into<String>) -> Self {
        Document {
            frontmatter,
            body: body.into(),
        }
    }

    /// Parses a document from raw file text.
    ///
    /// If the file does not begin with a `---` frontmatter delimiter, the
    /// entire text is treated as the body and the frontmatter is empty
    /// (matching the reference parser). An opened-but-unclosed frontmatter
    /// block is an error.
    pub fn parse(text: &str) -> Result<Document, DocumentError> {
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() || lines[0].trim() != FRONTMATTER_DELIM {
            return Ok(Document {
                frontmatter: Frontmatter::new(),
                body: text.to_string(),
            });
        }

        let mut end_idx = None;
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == FRONTMATTER_DELIM {
                end_idx = Some(i);
                break;
            }
        }
        let end_idx = end_idx.ok_or(DocumentError::UnterminatedFrontmatter)?;

        let fm_text = lines[1..end_idx].join("\n");
        let value = Value::parse(&fm_text)?;
        let frontmatter = match value {
            Value::Null => Frontmatter::new(),
            Value::Mapping(m) => Frontmatter::from_mapping(m),
            _ => return Err(DocumentError::FrontmatterNotMapping),
        };

        let mut body = lines[end_idx + 1..].join("\n");
        if let Some(stripped) = body.strip_prefix('\n') {
            body = stripped.to_string();
        }

        Ok(Document { frontmatter, body })
    }

    /// Serializes the document back to text: frontmatter delimited by `---`,
    /// a blank line, then the body (terminated by a newline).
    ///
    /// `parse` followed by `serialize` preserves frontmatter key order and the
    /// body (modulo trailing-newline normalization), matching the reference.
    pub fn serialize(&self) -> String {
        let fm_text = Value::Mapping(self.frontmatter.as_mapping().clone())
            .to_yaml_string()
            .trim_end()
            .to_string();
        let body = if self.body.ends_with('\n') {
            self.body.clone()
        } else {
            format!("{}\n", self.body)
        };
        format!("{FRONTMATTER_DELIM}\n{fm_text}\n{FRONTMATTER_DELIM}\n\n{body}")
    }

    /// Producer-side validation matching the reference `OKFDocument.validate`:
    /// requires `type`, `title`, `description`, and `timestamp` to all be
    /// present and non-empty.
    ///
    /// For spec **conformance** (§9), which requires only a non-empty `type`,
    /// use [`Document::validate_conformance`].
    pub fn validate(&self) -> Result<(), DocumentError> {
        let missing: Vec<String> = REQUIRED_FRONTMATTER_KEYS
            .iter()
            .filter(|k| {
                self.frontmatter
                    .get(k)
                    .map(Value::is_empty_value)
                    .unwrap_or(true)
            })
            .map(|k| k.to_string())
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(DocumentError::MissingKeys(missing))
        }
    }

    /// Spec-conformance validation (§9): the frontmatter must contain a
    /// non-empty `type` field. Optional fields are not required.
    pub fn validate_conformance(&self) -> Result<(), DocumentError> {
        let has_type = self
            .frontmatter
            .get("type")
            .map(|v| !v.is_empty_value())
            .unwrap_or(false);
        if has_type {
            Ok(())
        } else {
            Err(DocumentError::MissingKeys(vec!["type".to_string()]))
        }
    }

    /// Extracts all markdown links found in the body.
    pub fn links(&self) -> Vec<Link> {
        links::extract_links(&self.body)
    }

    /// Extracts numbered entries from the `# Citations` section, if present
    /// (§8).
    pub fn citations(&self) -> Vec<Citation> {
        links::extract_citations(&self.body)
    }
}
