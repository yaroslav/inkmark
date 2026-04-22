//! Table-of-contents rendering.
//!
//! Converts a slice of `TocEntry` values (produced by `stats::collect`)
//! into markdown or HTML strings.

use pulldown_cmark::HeadingLevel;
use pulldown_cmark_escape::escape_html;

pub struct TocEntry {
    pub level: HeadingLevel,
    pub text: String,
    pub slug: String,
}

/// Render TOC entries as a markdown list.
///
/// Square brackets in heading text are escaped (`[` → `\[`, `]` → `\]`) so
/// they don't break the markdown link syntax `[text](#slug)`.
///
/// When `max_depth` is `Some(n)`, only headings at level ≤ n are included.
/// `None` means no depth filtering (every heading appears).
pub fn toc_to_markdown(entries: &[TocEntry], max_depth: Option<u8>) -> String {
    let filtered: Vec<&TocEntry> = entries
        .iter()
        .filter(|e| max_depth.map_or(true, |max| level_to_u8(e.level) <= max))
        .collect();
    if filtered.is_empty() {
        return String::new();
    }
    let min_level = filtered
        .iter()
        .map(|e| level_to_u8(e.level))
        .min()
        .unwrap_or(1);

    let mut buf = String::new();
    for entry in &filtered {
        let indent = (level_to_u8(entry.level) - min_level) as usize * 2;
        for _ in 0..indent {
            buf.push(' ');
        }
        buf.push_str("- [");
        // Escape [ and ] so they don't break the markdown link syntax.
        for ch in entry.text.chars() {
            match ch {
                '[' => buf.push_str("\\["),
                ']' => buf.push_str("\\]"),
                c => buf.push(c),
            }
        }
        buf.push_str("](#");
        buf.push_str(&entry.slug);
        buf.push_str(")\n");
    }
    buf
}

/// Render TOC entries as a nested HTML `<ul>` list.
///
/// When `max_depth` is `Some(n)`, only headings at level ≤ n are included.
pub fn toc_to_html(entries: &[TocEntry], max_depth: Option<u8>) -> String {
    let filtered: Vec<&TocEntry> = entries
        .iter()
        .filter(|e| max_depth.map_or(true, |max| level_to_u8(e.level) <= max))
        .collect();
    if filtered.is_empty() {
        return String::new();
    }
    let min_level = filtered
        .iter()
        .map(|e| level_to_u8(e.level))
        .min()
        .unwrap_or(1);
    let mut buf = String::new();
    let mut open_levels: Vec<u8> = Vec::new();

    for entry in &filtered {
        let lvl = level_to_u8(entry.level);

        while open_levels
            .last()
            .copied()
            .unwrap_or(min_level.saturating_sub(1))
            >= lvl
        {
            buf.push_str("</li>\n</ul>\n");
            open_levels.pop();
        }

        while open_levels
            .last()
            .copied()
            .unwrap_or(min_level.saturating_sub(1))
            < lvl
        {
            buf.push_str("<ul>\n");
            open_levels.push(lvl);
        }

        buf.push_str("<li><a href=\"#");
        buf.push_str(&entry.slug);
        buf.push_str("\">");

        let _ = escape_html(&mut buf, &entry.text);
        buf.push_str("</a>\n");
    }

    while open_levels.pop().is_some() {
        buf.push_str("</li>\n</ul>\n");
    }

    buf
}

#[inline]
pub fn level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::HeadingLevel;

    fn entry(level: HeadingLevel, text: &str, slug: &str) -> TocEntry {
        TocEntry {
            level,
            text: text.to_string(),
            slug: slug.to_string(),
        }
    }

    #[test]
    fn empty_toc_returns_empty_string() {
        assert_eq!(toc_to_markdown(&[], None), "");
        assert_eq!(toc_to_html(&[], None), "");
    }

    #[test]
    fn markdown_escapes_brackets_in_heading_text() {
        let entries = vec![entry(HeadingLevel::H1, "Arrays [1..n]", "arrays-1-n")];
        let md = toc_to_markdown(&entries, None);
        assert!(md.contains("\\[1..n\\]"), "brackets must be escaped: {md}");
        assert!(md.contains("(#arrays-1-n)"));
    }

    #[test]
    fn markdown_simple_toc() {
        let entries = vec![
            entry(HeadingLevel::H1, "Introduction", "introduction"),
            entry(HeadingLevel::H2, "Background", "background"),
        ];
        let md = toc_to_markdown(&entries, None);
        assert_eq!(
            md,
            "- [Introduction](#introduction)\n  - [Background](#background)\n"
        );
    }

    #[test]
    fn html_toc_escapes_text() {
        let entries = vec![entry(HeadingLevel::H1, "A & B <C>", "a-b-c")];
        let html = toc_to_html(&entries, None);
        assert!(html.contains("A &amp; B &lt;C&gt;"));
    }

    #[test]
    fn max_depth_filters_markdown() {
        let entries = vec![
            entry(HeadingLevel::H1, "Top", "top"),
            entry(HeadingLevel::H2, "Mid", "mid"),
            entry(HeadingLevel::H3, "Deep", "deep"),
        ];
        let md = toc_to_markdown(&entries, Some(2));
        assert!(md.contains("[Top]"));
        assert!(md.contains("[Mid]"));
        assert!(!md.contains("[Deep]"));
    }

    #[test]
    fn max_depth_filters_html() {
        let entries = vec![
            entry(HeadingLevel::H1, "Top", "top"),
            entry(HeadingLevel::H2, "Mid", "mid"),
            entry(HeadingLevel::H3, "Deep", "deep"),
        ];
        let html = toc_to_html(&entries, Some(2));
        assert!(html.contains(">Top</a>"));
        assert!(html.contains(">Mid</a>"));
        assert!(!html.contains(">Deep</a>"));
    }

    #[test]
    fn max_depth_one_keeps_only_h1() {
        let entries = vec![
            entry(HeadingLevel::H1, "Top", "top"),
            entry(HeadingLevel::H2, "Mid", "mid"),
        ];
        let md = toc_to_markdown(&entries, Some(1));
        assert_eq!(md, "- [Top](#top)\n");
    }

    #[test]
    fn max_depth_respects_remaining_entries_for_min_level() {
        // With h3 filtered out, min_level is h1, so h2 gets 2-space indent.
        let entries = vec![
            entry(HeadingLevel::H1, "Top", "top"),
            entry(HeadingLevel::H2, "Mid", "mid"),
            entry(HeadingLevel::H3, "Deep", "deep"),
        ];
        let md = toc_to_markdown(&entries, Some(2));
        assert_eq!(md, "- [Top](#top)\n  - [Mid](#mid)\n");
    }
}
