//! Loading and traversing an OKF *bundle*: a directory tree of markdown files
//! (§3).
//!
//! [`Bundle::load`] walks a directory, parses every non-reserved `.md` file
//! into a [`Concept`], records the reserved `index.md` / `log.md` files, and
//! builds the cross-link graph (§5). Loading is **permissive** by design (§9):
//! files whose frontmatter cannot be parsed are collected into
//! [`Bundle::parse_errors`] rather than aborting the load, and broken links are
//! retained as edges to non-existent concepts.

use crate::concept_id::ConceptId;
use crate::document::Document;
use crate::error::{BundleError, DocumentError};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Reserved filenames with defined meaning at any level (§3.1).
pub const RESERVED_FILENAMES: [&str; 2] = ["index.md", "log.md"];

/// A single concept within a bundle (one markdown document).
#[derive(Clone, Debug)]
pub struct Concept {
    /// The concept's id (path minus `.md`).
    pub id: ConceptId,
    /// The file path on disk.
    pub path: PathBuf,
    /// The parsed document.
    pub document: Document,
}

/// A cross-link from one concept to another, after resolution (§5.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedLink {
    /// The concept the link points at.
    pub target: ConceptId,
    /// Whether the target concept exists in the bundle (a `false` is allowed —
    /// broken links are not malformed).
    pub exists: bool,
    /// The link text.
    pub text: String,
    /// The raw link target as written.
    pub raw: String,
}

/// A loaded OKF bundle.
#[derive(Debug)]
pub struct Bundle {
    root: PathBuf,
    concepts: Vec<Concept>,
    index: HashMap<ConceptId, usize>,
    index_files: Vec<PathBuf>,
    log_files: Vec<PathBuf>,
    parse_errors: Vec<(PathBuf, DocumentError)>,
    outbound: HashMap<ConceptId, Vec<ResolvedLink>>,
    backlinks: HashMap<ConceptId, Vec<ConceptId>>,
}

impl Bundle {
    /// Loads a bundle from a directory tree.
    ///
    /// Returns an error only for I/O failures or a non-directory root. Per-file
    /// parse failures are recorded in [`Bundle::parse_errors`].
    pub fn load(root: impl AsRef<Path>) -> Result<Bundle, BundleError> {
        let root = root.as_ref().to_path_buf();
        if !root.is_dir() {
            return Err(BundleError::NotADirectory(root));
        }

        let mut md_files = Vec::new();
        collect_markdown(&root, &mut md_files)?;
        md_files.sort();

        let mut concepts = Vec::new();
        let mut index_files = Vec::new();
        let mut log_files = Vec::new();
        let mut parse_errors = Vec::new();

        for path in md_files {
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            match filename.as_str() {
                "index.md" => index_files.push(path),
                "log.md" => log_files.push(path),
                _ => {
                    let text = fs::read_to_string(&path)?;
                    match Document::parse(&text) {
                        Ok(document) => match ConceptId::from_path(&root, &path) {
                            Ok(id) => concepts.push(Concept { id, path, document }),
                            Err(e) => parse_errors.push((
                                path,
                                DocumentError::MissingKeys(vec![e.to_string()]),
                            )),
                        },
                        Err(e) => parse_errors.push((path, e)),
                    }
                }
            }
        }

        let mut index = HashMap::new();
        for (i, c) in concepts.iter().enumerate() {
            index.insert(c.id.clone(), i);
        }

        let (outbound, backlinks) = build_graph(&concepts, &index);

        Ok(Bundle {
            root,
            concepts,
            index,
            index_files,
            log_files,
            parse_errors,
            outbound,
            backlinks,
        })
    }

    /// The bundle's root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// All successfully parsed concepts, in path order.
    pub fn concepts(&self) -> &[Concept] {
        &self.concepts
    }

    /// Number of concepts.
    pub fn len(&self) -> usize {
        self.concepts.len()
    }

    /// `true` if the bundle has no concepts.
    pub fn is_empty(&self) -> bool {
        self.concepts.is_empty()
    }

    /// Looks up a concept by id.
    pub fn get(&self, id: &ConceptId) -> Option<&Concept> {
        self.index.get(id).map(|&i| &self.concepts[i])
    }

    /// `true` if a concept with this id exists.
    pub fn contains(&self, id: &ConceptId) -> bool {
        self.index.contains_key(id)
    }

    /// Paths of all `index.md` files found (§6).
    pub fn index_files(&self) -> &[PathBuf] {
        &self.index_files
    }

    /// Paths of all `log.md` files found (§7).
    pub fn log_files(&self) -> &[PathBuf] {
        &self.log_files
    }

    /// Files whose frontmatter could not be parsed during loading.
    pub fn parse_errors(&self) -> &[(PathBuf, DocumentError)] {
        &self.parse_errors
    }

    /// The resolved outbound cross-links from a concept.
    pub fn links_from(&self, id: &ConceptId) -> &[ResolvedLink] {
        self.outbound.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The ids of concepts that link to the given concept ("cited by" / §
    /// backlinks).
    pub fn backlinks(&self, id: &ConceptId) -> &[ConceptId] {
        self.backlinks.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// All broken internal links in the bundle, as `(source, raw_target)`
    /// pairs. Broken links are permitted by the spec (§5.3) — this is
    /// informational.
    pub fn broken_links(&self) -> Vec<(ConceptId, String)> {
        let mut out = Vec::new();
        for c in &self.concepts {
            for link in self.links_from(&c.id) {
                if !link.exists {
                    out.push((c.id.clone(), link.raw.clone()));
                }
            }
        }
        out
    }

    /// The declared OKF version from the bundle-root `index.md` frontmatter, if
    /// present (`okf_version`, §11). This is the only place frontmatter is
    /// permitted in an `index.md`.
    pub fn okf_version(&self) -> Option<String> {
        let root_index = self.root.join("index.md");
        let text = fs::read_to_string(&root_index).ok()?;
        let doc = Document::parse(&text).ok()?;
        doc.frontmatter
            .get("okf_version")
            .and_then(crate::yaml::Value::as_display_string)
    }
}

/// Recursively collects `*.md` file paths under `dir`.
fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), BundleError> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_markdown(&path, out)?;
        } else if file_type.is_file()
            && path.extension().map(|e| e == "md").unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Builds the outbound link and backlink maps for all concepts.
fn build_graph(
    concepts: &[Concept],
    index: &HashMap<ConceptId, usize>,
) -> (
    HashMap<ConceptId, Vec<ResolvedLink>>,
    HashMap<ConceptId, Vec<ConceptId>>,
) {
    let mut outbound: HashMap<ConceptId, Vec<ResolvedLink>> = HashMap::new();
    let mut backlinks: HashMap<ConceptId, Vec<ConceptId>> = HashMap::new();

    for c in concepts {
        let mut resolved = Vec::new();
        for link in c.document.links() {
            if let Some(target) = link.resolve(&c.id) {
                let exists = index.contains_key(&target);
                if exists {
                    let entry = backlinks.entry(target.clone()).or_default();
                    if !entry.contains(&c.id) {
                        entry.push(c.id.clone());
                    }
                }
                resolved.push(ResolvedLink {
                    target,
                    exists,
                    text: link.text,
                    raw: link.target,
                });
            }
        }
        outbound.insert(c.id.clone(), resolved);
    }

    (outbound, backlinks)
}
