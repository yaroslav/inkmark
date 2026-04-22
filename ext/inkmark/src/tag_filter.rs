//! GFM "Disallowed Raw HTML" extension (spec §6.11).
//!
//! Escapes the leading `<` of nine spec-designated tag names so raw
//! HTML that would change how the document is parsed (or run script)
//! renders as text instead. Mirrors [comrak](https://github.com/kivikakk/comrak/blob/main/src/html.rs): the
//! transformation is defined textually by the GFM spec, so we do a
//! byte scan rather than parse HTML.

use pulldown_cmark::{CowStr, Event};

const DISALLOWED: &[&[u8]] = &[
    b"title",
    b"textarea",
    b"style",
    b"xmp",
    b"iframe",
    b"noembed",
    b"noframes",
    b"script",
    b"plaintext",
];

/// Apply the tagfilter to a single event. If the event needs no
/// rewrite, it's returned unchanged.
#[inline]
pub fn apply_event(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Html(s) => match rewrite(&s) {
            Some(out) => Event::Html(CowStr::Boxed(out.into_boxed_str())),
            None => Event::Html(s),
        },
        Event::InlineHtml(s) => match rewrite(&s) {
            Some(out) => Event::InlineHtml(CowStr::Boxed(out.into_boxed_str())),
            None => Event::InlineHtml(s),
        },
        other => other,
    }
}

/// Byte-scan `input` for disallowed tag opens/closes. Returns
/// `Some(new_string)` when at least one rewrite happened; `None`
/// when the input is already clean so callers can skip the clone.
fn rewrite(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out: Option<String> = None;
    let mut scan_start = 0;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' && is_disallowed_at(bytes, i) {
            let s = out.get_or_insert_with(|| String::with_capacity(input.len() + 12));
            s.push_str(&input[scan_start..i]);
            s.push_str("&lt;");
            scan_start = i + 1;
        }
        i += 1;
    }

    out.map(|mut s| {
        s.push_str(&input[scan_start..]);
        s
    })
}

/// True when `bytes[pos..]` starts with `<` or `</`, followed by one
/// of the disallowed tag names, with the next char being a proper
/// tag-boundary (space, tab, CR, LF, `>`, or `/>`).
fn is_disallowed_at(bytes: &[u8], pos: usize) -> bool {
    debug_assert_eq!(bytes[pos], b'<');
    let mut i = pos + 1;
    if i >= bytes.len() {
        return false;
    }
    if bytes[i] == b'/' {
        i += 1;
        if i >= bytes.len() {
            return false;
        }
    }

    for &name in DISALLOWED {
        let end = i + name.len();
        if end > bytes.len() {
            continue;
        }
        if !bytes[i..end].eq_ignore_ascii_case(name) {
            continue;
        }
        // Require a proper tag-boundary so `<scripter>` doesn't match.
        if end == bytes.len() {
            // Ambiguous cut-off: match comrak's conservative default
            // (no escape).
            return false;
        }
        let next = bytes[end];
        if is_space(next) || next == b'>' {
            return true;
        }
        if next == b'/' {
            // Match only when `/>` (spec's self-closing form).
            return end + 1 < bytes.len() && bytes[end + 1] == b'>';
        }
        return false;
    }
    false
}

/// ASCII whitespace as defined by cmark's `isspace`: space, tab, CR, LF.
/// Matches comrak byte-for-byte.
#[inline]
fn is_space(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\r' || c == b'\n'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rw(s: &str) -> String {
        rewrite(s).unwrap_or_else(|| s.to_string())
    }

    #[test]
    fn escapes_open_tag() {
        assert_eq!(rw("<script>"), "&lt;script>");
    }

    #[test]
    fn escapes_close_tag() {
        assert_eq!(rw("</script>"), "&lt;/script>");
    }

    #[test]
    fn escapes_both_in_one_pass() {
        assert_eq!(
            rw("hi <script>alert(1)</script> bye"),
            "hi &lt;script>alert(1)&lt;/script> bye"
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(rw("<SCRIPT>"), "&lt;SCRIPT>");
        assert_eq!(rw("<ScRiPt>"), "&lt;ScRiPt>");
        assert_eq!(rw("</IFRAME>"), "&lt;/IFRAME>");
    }

    #[test]
    fn does_not_match_prefix() {
        assert_eq!(rw("<scripter>"), "<scripter>");
        assert_eq!(rw("<styles>"), "<styles>");
        assert_eq!(rw("<titleish>"), "<titleish>");
    }

    #[test]
    fn escapes_with_attributes() {
        assert_eq!(
            rw(r#"<script src="evil.js">"#),
            r#"&lt;script src="evil.js">"#
        );
        assert_eq!(rw("<iframe\tsrc=\"x\">"), "&lt;iframe\tsrc=\"x\">");
    }

    #[test]
    fn escapes_self_closing() {
        assert_eq!(rw("<script/>"), "&lt;script/>");
    }

    #[test]
    fn non_self_closing_slash_not_escaped() {
        // `<script/ok>` is weird; comrak's rule requires `/>` exactly.
        assert_eq!(rw("<script/ok>"), "<script/ok>");
    }

    #[test]
    fn all_nine_tags_escaped() {
        for name in [
            "title",
            "textarea",
            "style",
            "xmp",
            "iframe",
            "noembed",
            "noframes",
            "script",
            "plaintext",
        ] {
            let input = format!("<{name}>");
            let expected = format!("&lt;{name}>");
            assert_eq!(rw(&input), expected, "tag: {name}");
        }
    }

    #[test]
    fn no_alloc_when_clean() {
        assert!(rewrite("<b>hi</b>").is_none());
        assert!(rewrite("plain text").is_none());
        assert!(rewrite("").is_none());
    }

    #[test]
    fn handles_cut_off_at_end() {
        // No trailing boundary char—ambiguous, don't escape.
        assert_eq!(rw("<script"), "<script");
        assert_eq!(rw("</script"), "</script");
    }

    #[test]
    fn standalone_lt_passes_through() {
        assert_eq!(rw("< script>"), "< script>");
        assert_eq!(rw("a < b"), "a < b");
    }

    #[test]
    fn already_escaped_not_double_escaped() {
        assert_eq!(rw("&lt;script>"), "&lt;script>");
    }

    #[test]
    fn matches_comrak_reference_case() {
        // From comrak/src/tests/tagfilter.rs: "hi <xmp> ok\n\n<xmp>\n"
        let input = "hi <xmp> ok\n\n<xmp>\n";
        let expected = "hi &lt;xmp> ok\n\n&lt;xmp>\n";
        assert_eq!(rw(input), expected);
    }
}
