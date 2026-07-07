//! Markdown link extraction, classification, and citation parsing (§5, §8).
//!
//! OKF relationships are expressed as ordinary markdown links, so this module
//! provides a small, dependency-free scanner for inline `[text](dest)` links
//! plus the link-classification rules from §5 (absolute bundle-relative vs.
//! relative vs. external). It ignores links inside fenced code blocks and
//! inline code spans, which are content rather than relationships.

use crate::concept_id::ConceptId;

/// How a link target is interpreted under §5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// Begins with `/`: resolved relative to the bundle root (§5.1, recommended).
    Absolute,
    /// A relative path such as `./other.md` (§5.2).
    Relative,
    /// An external URI (`https://…`, `mailto:…`, …).
    External,
    /// A pure in-document anchor (`#section`).
    Anchor,
    /// Anything else (e.g. an empty target).
    Other,
}

/// A markdown link found in a concept body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The link text (between `[` and `]`).
    pub text: String,
    /// The raw destination (between `(` and `)`), with any title removed.
    pub target: String,
    /// The classification of [`Link::target`].
    pub kind: LinkKind,
}

impl Link {
    /// Classifies a raw target string per §5.
    pub fn classify(target: &str) -> LinkKind {
        let t = target.trim();
        if t.is_empty() {
            LinkKind::Other
        } else if t.starts_with('#') {
            LinkKind::Anchor
        } else if is_external(t) {
            LinkKind::External
        } else if t.starts_with('/') {
            LinkKind::Absolute
        } else {
            LinkKind::Relative
        }
    }

    /// Resolves an internal link to the concept id it points at, given the id
    /// of the concept the link appears in.
    ///
    /// Returns `None` for external links, anchors, links to directories
    /// (targets ending in `/`), or targets that cannot form a valid concept id.
    /// The result is *not* guaranteed to exist in the bundle — broken links are
    /// permitted by the spec (§5.3).
    pub fn resolve(&self, source: &ConceptId) -> Option<ConceptId> {
        match self.kind {
            LinkKind::Absolute => resolve_absolute(&self.target),
            LinkKind::Relative => resolve_relative(&self.target, source),
            _ => None,
        }
    }
}

/// A numbered entry under the `# Citations` heading (§8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citation {
    /// The citation number (the `n` in `[n]`).
    pub number: u32,
    /// The link text, if the entry is a markdown link.
    pub text: Option<String>,
    /// The cited URL/target, if present.
    pub target: Option<String>,
    /// The full raw text of the entry after the `[n]` marker.
    pub raw: String,
}

fn is_external(t: &str) -> bool {
    let lower = t.to_ascii_lowercase();
    lower.starts_with("//") // protocol-relative URL
        || lower.contains("://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with("data:")
}

fn strip_anchor(target: &str) -> &str {
    match target.find('#') {
        Some(i) => &target[..i],
        None => target,
    }
}

/// Percent-decodes a single path component (e.g. `Quarterly%20Report` →
/// `Quarterly Report`), so links that encode spaces the canonical markdown way
/// resolve to the concept whose filename contains a literal space. Malformed or
/// incomplete `%` escapes are left untouched.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn resolve_absolute(target: &str) -> Option<ConceptId> {
    let t = strip_anchor(target);
    if t.ends_with('/') {
        return None; // directory link
    }
    // Normalize `.`/`..` segments relative to the bundle root, consistent with
    // relative-link resolution.
    let mut segs: Vec<String> = Vec::new();
    for comp in t.trim_start_matches('/').split('/') {
        match comp {
            "" | "." => continue,
            ".." => {
                segs.pop();
            }
            other => segs.push(percent_decode(other)),
        }
    }
    if let Some(last) = segs.last_mut() {
        if let Some(s) = last.strip_suffix(".md") {
            *last = s.to_string();
        }
    }
    ConceptId::new(segs).ok()
}

fn resolve_relative(target: &str, source: &ConceptId) -> Option<ConceptId> {
    let t = strip_anchor(target);
    if t.is_empty() || t.ends_with('/') {
        return None;
    }
    // Start from the source concept's directory.
    let mut segs: Vec<String> = match source.parent() {
        Some(p) => p.segments().to_vec(),
        None => Vec::new(),
    };
    for comp in t.split('/') {
        match comp {
            "" | "." => continue,
            ".." => {
                segs.pop();
            }
            other => segs.push(percent_decode(other)),
        }
    }
    if let Some(last) = segs.last_mut() {
        if let Some(s) = last.strip_suffix(".md") {
            *last = s.to_string();
        }
    }
    ConceptId::new(segs).ok()
}

/// Extracts all inline markdown links from a body, skipping fenced code blocks
/// and inline code spans.
pub fn extract_links(body: &str) -> Vec<Link> {
    let mut links = Vec::new();
    for line in code_free_lines(body) {
        scan_line_links(&line, &mut links);
    }
    links
}

/// Returns the body's lines with fenced code blocks removed and inline code
/// spans blanked out.
fn code_free_lines(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fence: Option<char> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(f) = fence {
            // Inside a fence; look for the closing marker.
            if trimmed.starts_with(&f.to_string().repeat(3)) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            fence = Some('`');
            continue;
        }
        if trimmed.starts_with("~~~") {
            fence = Some('~');
            continue;
        }
        out.push(blank_inline_code(line));
    }
    out
}

/// Replaces inline code spans (backtick-delimited) with spaces so links inside
/// them are not extracted.
fn blank_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if in_code {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Scans a single (code-free) line for `[text](dest)` links.
fn scan_line_links(line: &str, out: &mut Vec<Link>) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some((text, dest, next)) = parse_inline_link(&chars, i) {
                let target = strip_title(&dest);
                out.push(Link {
                    text,
                    kind: Link::classify(&target),
                    target,
                });
                i = next;
                continue;
            }
        }
        i += 1;
    }
}

/// Attempts to parse `[text](dest)` starting at `start` (the `[`). Returns the
/// text, destination, and index just past the closing `)`.
fn parse_inline_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    // Match the link text up to a balanced `]`.
    let mut i = start + 1;
    let mut depth = 1;
    let text_start = i;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1, // skip escaped char
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if depth != 0 || i >= chars.len() {
        return None;
    }
    let text: String = chars[text_start..i].iter().collect();
    // Next non-space char must be '('.
    let mut j = i + 1;
    if j >= chars.len() || chars[j] != '(' {
        return None;
    }
    j += 1;
    let dest_start = j;
    let mut paren = 1;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 1,
            '(' => paren += 1,
            ')' => {
                paren -= 1;
                if paren == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if paren != 0 || j >= chars.len() {
        return None;
    }
    let dest: String = chars[dest_start..j].iter().collect();
    Some((text, dest, j + 1))
}

/// Removes an optional `"title"` (or `'title'`) suffix from a link destination.
fn strip_title(dest: &str) -> String {
    let d = dest.trim();
    if let Some(idx) = d.find([' ', '\t']) {
        let (url, rest) = d.split_at(idx);
        let rest = rest.trim_start();
        if rest.starts_with('"') || rest.starts_with('\'') {
            return url.to_string();
        }
    }
    d.to_string()
}

/// Extracts numbered citation entries from the `# Citations` section (§8).
pub fn extract_citations(body: &str) -> Vec<Citation> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            let title = heading.trim_start_matches('#').trim();
            if in_section {
                // A new heading ends the citations section.
                break;
            }
            in_section = title.eq_ignore_ascii_case("citations");
            continue;
        }
        if !in_section || trimmed.is_empty() {
            continue;
        }
        if let Some(cit) = parse_citation_line(trimmed) {
            out.push(cit);
        }
    }
    out
}

/// Parses a single `[n] …` citation line.
fn parse_citation_line(line: &str) -> Option<Citation> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let number: u32 = rest[..close].trim().parse().ok()?;
    let after = rest[close + 1..].trim().to_string();

    // If the remainder is itself a markdown link, capture its text and target.
    let mut text = None;
    let mut target = None;
    let chars: Vec<char> = after.chars().collect();
    if let Some(open) = chars.iter().position(|&c| c == '[') {
        if let Some((t, dest, _)) = parse_inline_link(&chars, open) {
            text = Some(t);
            target = Some(strip_title(&dest));
        }
    }
    Some(Citation {
        number,
        text,
        target,
        raw: after,
    })
}
