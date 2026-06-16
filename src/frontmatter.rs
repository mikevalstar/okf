//! Typed, order-preserving access to a concept's YAML frontmatter.
//!
//! OKF frontmatter is an open mapping: a few well-known keys (§4.1 of the
//! [spec]) plus arbitrary producer-defined extensions that consumers MUST
//! preserve when round-tripping. [`Frontmatter`] therefore stores the full
//! [`Mapping`] verbatim and layers typed accessors on top, rather than
//! deserializing into a fixed struct that would drop unknown keys.
//!
//! [spec]: https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md

use crate::yaml::{Mapping, Value};

/// Frontmatter keys the reference enrichment agent requires before a document
/// is considered publishable (its `OKFDocument.validate()`). Note this is
/// *stricter* than spec conformance (§9), which requires only `type`.
pub const REQUIRED_FRONTMATTER_KEYS: [&str; 4] = ["type", "title", "description", "timestamp"];

/// A concept's frontmatter: an ordered key/value mapping with typed accessors
/// for the well-known OKF fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frontmatter {
    map: Mapping,
}

impl Frontmatter {
    /// Creates an empty frontmatter block.
    pub fn new() -> Self {
        Frontmatter {
            map: Mapping::new(),
        }
    }

    /// Wraps an existing mapping.
    pub fn from_mapping(map: Mapping) -> Self {
        Frontmatter { map }
    }

    /// Borrows the underlying ordered mapping.
    pub fn as_mapping(&self) -> &Mapping {
        &self.map
    }

    /// Mutably borrows the underlying ordered mapping.
    pub fn as_mapping_mut(&mut self) -> &mut Mapping {
        &mut self.map
    }

    /// Consumes the wrapper, returning the underlying mapping.
    pub fn into_mapping(self) -> Mapping {
        self.map
    }

    /// `true` if there are no keys.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Raw value for an arbitrary key (including producer extensions).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }

    /// Sets a raw value for a key, preserving position if it already exists.
    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.map.insert(key, value);
    }

    /// The **required** `type` field (§4.1). `None` if absent or not a scalar.
    pub fn type_(&self) -> Option<String> {
        self.map.get("type").and_then(Value::as_display_string)
    }

    /// The optional `title` field.
    pub fn title(&self) -> Option<String> {
        self.map.get("title").and_then(Value::as_display_string)
    }

    /// The optional one-line `description`.
    pub fn description(&self) -> Option<String> {
        self.map.get("description").and_then(Value::as_display_string)
    }

    /// The optional `resource` URI for the underlying asset.
    pub fn resource(&self) -> Option<String> {
        self.map.get("resource").and_then(Value::as_display_string)
    }

    /// The optional ISO-8601 `timestamp` of last meaningful change.
    pub fn timestamp(&self) -> Option<String> {
        self.map.get("timestamp").and_then(Value::as_display_string)
    }

    /// The optional `tags` list. Non-string elements are coerced to their
    /// display form; a non-sequence `tags` value yields an empty vector.
    pub fn tags(&self) -> Vec<String> {
        match self.map.get("tags") {
            Some(Value::Sequence(items)) => items
                .iter()
                .filter_map(Value::as_display_string)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Returns the keys present that are not well-known OKF fields — i.e. the
    /// producer-defined extension keys consumers must preserve (§4.1).
    pub fn extension_keys(&self) -> Vec<&str> {
        const KNOWN: [&str; 6] = ["type", "title", "description", "resource", "tags", "timestamp"];
        self.map
            .keys()
            .filter(|k| !KNOWN.contains(k))
            .collect()
    }
}

impl From<Mapping> for Frontmatter {
    fn from(map: Mapping) -> Self {
        Frontmatter { map }
    }
}
