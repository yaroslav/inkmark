//! Syntax highlighting filter for fenced code blocks.
//!
//! When enabled, intercepts fenced code blocks that have an explicit language
//! tag (e.g. ````rust`), runs the code through syntect's
//! `ClassedHTMLGenerator`, and replaces the original
//! `Start(CodeBlock) / Text / End(CodeBlock)` event sequence with a
//! single `Event::Html` carrying the highlighted markup.
//!
//! Code blocks without a language tag (bare ```` ``` ````) and indented code
//! blocks are left alone (no language specified).
//!
//! The output uses CSS class names (via `ClassStyle::Spaced`).

use std::sync::OnceLock;

use magnus::{Error, Ruby};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};
use syntect::highlighting::ThemeSet;
use syntect::html::{css_for_theme_with_class_style, ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Process-lifetime cache for the default syntax set. Loading the embedded
/// syntax definitions takes ~100-200ms on first call.
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Replace fenced code blocks that have a language tag with syntect-
/// highlighted HTML. Blocks without a language and indented code blocks
/// pass through unchanged.
pub fn highlight(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let ss = syntax_set();
    let mut out: Vec<Event<'_>> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();

    while let Some(event) = iter.next() {
        match &event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) if !lang.is_empty() => {
                let lang_str = lang.to_string();

                // Consume text events until End(CodeBlock).
                let mut code = String::new();
                for inner in iter.by_ref() {
                    match inner {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(t) => code.push_str(&t),
                        _ => {}
                    }
                }

                let html = highlight_code(&code, &lang_str, ss);
                out.push(Event::Html(CowStr::Boxed(html.into_boxed_str())));
            }
            _ => out.push(event),
        }
    }

    out
}

/// Run syntect on a code string with the given language hint. Returns a
/// complete `<pre><code class="language-{lang}">...highlighted...</code></pre>`
/// block. If the language isn't recognized, falls back to plain-text grammar.
#[inline]
fn highlight_code(code: &str, lang: &str, ss: &SyntaxSet) -> String {
    let syntax = ss
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let mut gen = ClassedHTMLGenerator::new_with_class_style(syntax, ss, ClassStyle::Spaced);

    for line in LinesWithEndings::from(code) {
        // parse_html_for_line_which_includes_newline can return Err on
        // malformed syntax definitions. Swallow the error and stop highlighting this block.
        if gen
            .parse_html_for_line_which_includes_newline(line)
            .is_err()
        {
            break;
        }
    }

    let highlighted = gen.finalize();

    // Wrap each line in <span class="line"> so CSS can add line numbers
    // via counter()/::before, highlight specific lines on hover, etc.
    let mut buf = format!("<pre><code class=\"language-{lang}\">");
    for line in highlighted.split('\n') {
        if !line.is_empty() {
            buf.push_str("<span class=\"line\">");
            buf.push_str(line);
            buf.push_str("</span>\n");
        }
    }
    buf.push_str("</code></pre>");
    buf
}

/// Default theme name for CSS generation.
const DEFAULT_THEME: &str = "base16-ocean.dark";

static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Quality of life helper.
/// Return CSS that styles the `<span class="...">` tokens produced by
/// `highlight()`. Accepts an optional theme name; defaults to
/// "base16-ocean.dark" when nil. The CSS string is suitable for embedding
/// in a `<style>` tag or writing to a `.css` file.
pub fn syntax_css(ruby: &Ruby, theme_name: Option<String>) -> Result<String, Error> {
    let ts = theme_set();
    let name = theme_name.as_deref().unwrap_or(DEFAULT_THEME);
    let theme = ts.themes.get(name).ok_or_else(|| {
        let available: Vec<&str> = ts.themes.keys().map(|s| s.as_str()).collect();
        Error::new(
            ruby.exception_arg_error(),
            format!("unknown syntax theme '{name}'. Available: {available:?}"),
        )
    })?;
    css_for_theme_with_class_style(theme, ClassStyle::Spaced).map_err(|e| {
        Error::new(
            ruby.exception_runtime_error(),
            format!("failed to generate CSS: {e}"),
        )
    })
}

/// Return an array of available theme names.
pub fn syntax_themes() -> Vec<String> {
    theme_set().themes.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{CodeBlockKind, CowStr, Event, Tag, TagEnd};

    #[test]
    fn highlight_rust_code() {
        let html = highlight_code("let x = 1;\n", "rust", syntax_set());
        assert!(html.contains("<span"), "should contain span tags: {html}");
        assert!(html.contains("language-rust"));
        assert!(html.contains("<pre><code"));
    }

    #[test]
    fn unknown_language_falls_back_to_plain_text() {
        let html = highlight_code("hello\n", "nonexistent-lang-xyz", syntax_set());
        // Plain text grammar produces no <span> tags—just escaped text.
        assert!(html.contains("hello"));
        assert!(html.contains("<pre><code"));
    }

    #[test]
    fn highlight_filter_replaces_fenced_block() {
        let events = vec![
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(CowStr::Borrowed(
                "rust",
            )))),
            Event::Text(CowStr::Borrowed("let x = 1;\n")),
            Event::End(TagEnd::CodeBlock),
        ];
        let out = highlight(events);
        assert_eq!(out.len(), 1);
        match &out[0] {
            Event::Html(html) => {
                assert!(html.contains("<span"), "missing spans: {html}");
                assert!(html.contains("language-rust"));
            }
            other => panic!("expected Html event, got {other:?}"),
        }
    }

    #[test]
    fn highlight_filter_skips_blocks_without_language() {
        let events = vec![
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(CowStr::Borrowed("")))),
            Event::Text(CowStr::Borrowed("plain\n")),
            Event::End(TagEnd::CodeBlock),
        ];
        let out = highlight(events);
        // Should pass through unchanged (3 events, not collapsed to 1)
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn highlight_filter_skips_indented_blocks() {
        let events = vec![
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)),
            Event::Text(CowStr::Borrowed("indented\n")),
            Event::End(TagEnd::CodeBlock),
        ];
        let out = highlight(events);
        assert_eq!(out.len(), 3);
    }
}
