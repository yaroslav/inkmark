//! Image attribute injection filter and URL matcher.
//!
//! When enabled, replaces pulldown-cmark's default image event sequence
//! (`Start(Tag::Image) ... End(TagEnd::Image)`) with a single `Event::InlineHtml`
//! carrying a hand-built `<img>` tag that includes the "modern" loading and
//! decoding hints:
//!
//! ```html
//! <img src="..." alt="..." loading="lazy" decoding="async" />
//! ```
//!
//! Pulldown-cmark's `Tag::Image` struct doesn't expose an "extra attributes"
//! field, so rewriting the Tag in place isn't enough—we have to bypass
//! the built-in image writer entirely and emit the HTML ourselves. Alt,
//! title, and URL are escaped through the same `pulldown-cmark-escape`
//! functions the upstream html writer uses, so the output stays byte-
//! compatible with what pulldown-cmark would have produced plus the two
//! extra attributes.

use globset::GlobSet;
use pulldown_cmark::{CowStr, Event, Tag, TagEnd};
use pulldown_cmark_escape::{escape_href, escape_html};

use crate::url_match::is_host_allowed;

/// Rewrite every image in the event stream as a self-contained `Event::InlineHtml`
/// carrying `<img ... loading="lazy" decoding="async">`.
///
/// We consume the input Vec to own each event, then rebuild with
/// `Vec::with_capacity(events.len())` so passthrough events move by value
/// and image events are replaced with a single Html event.
pub fn add_lazy_loading(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut out: Vec<Event<'_>> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let alt = take_alt_text(&mut iter);
                let html = build_img_tag(&dest_url, &alt, &title);
                out.push(Event::InlineHtml(CowStr::Boxed(html.into_boxed_str())));
            }
            other => out.push(other),
        }
    }

    out
}

/// Drop images whose `src` host isn't in the allowlist. The whole
/// `Start(Image) ... End(Image)` sequence is replaced with a single
/// `Event::Text` carrying the image's alt text, or removed entirely
/// when alt is empty. Non-web URLs pass through: [`is_host_allowed`]
/// returns true for any URL with no parseable host.
///
/// Alt accumulation is shared with `add_lazy_loading` (see
/// [`take_alt_text`]).
pub fn filter_by_hosts<'a>(events: Vec<Event<'a>>, set: &GlobSet) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Image { ref dest_url, .. }) if !is_host_allowed(dest_url, set) => {
                let alt = take_alt_text(&mut iter);
                if !alt.is_empty() {
                    out.push(Event::Text(CowStr::Boxed(alt.into_boxed_str())));
                }
            }
            other => out.push(other),
        }
    }

    out
}

/// Consume events through the `End(Image)` matching an already-consumed
/// `Start(Image)` and return the alt text: the raw Text and Code payloads,
/// with formatting (`![**bold**](img.png)`) discarded. An image nested in
/// the alt text (`![a ![b](x)](y)`) contributes its text too, so depth is
/// counted to stop at the matching End rather than the nested image's.
fn take_alt_text<'a>(iter: &mut impl Iterator<Item = Event<'a>>) -> String {
    let mut alt = String::new();
    let mut depth: usize = 1;
    for inner in iter {
        match inner {
            Event::Start(Tag::Image { .. }) => depth += 1,
            Event::End(TagEnd::Image) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Event::Text(t) | Event::Code(t) => alt.push_str(&t),
            _ => {}
        }
    }
    alt
}

/// Construct the `<img>` HTML string with `loading="lazy"` and
/// `decoding="async"` attributes. `src` is escaped as a URL (percent-
/// encoded where necessary); `alt` and `title` are HTML-attribute
/// escaped. The output matches pulldown-cmark's built-in image writer
/// plus the two extra hint attributes.
#[inline]
fn build_img_tag(src: &str, alt: &str, title: &str) -> String {
    // Rough capacity estimate: base tag (~60) + src + alt + title length.
    let mut out = String::with_capacity(60 + src.len() + alt.len() + title.len());
    out.push_str("<img src=\"");

    // escape_href percent-encodes problematic bytes and also handles HTML
    // specials (&, <, etc.). Matches pulldown-cmark's upstream behavior.
    let _ = escape_href(&mut out, src);
    out.push_str("\" alt=\"");
    let _ = escape_html(&mut out, alt);
    out.push('"');
    if !title.is_empty() {
        out.push_str(" title=\"");
        let _ = escape_html(&mut out, title);
        out.push('"');
    }
    out.push_str(" loading=\"lazy\" decoding=\"async\" />");
    out
}

#[cfg(test)]
mod tests {
    use super::{add_lazy_loading, build_img_tag, filter_by_hosts};
    use globset::{Glob, GlobSetBuilder};
    use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

    fn host_set(patterns: &[&str]) -> globset::GlobSet {
        let mut b = GlobSetBuilder::new();
        for p in patterns {
            b.add(Glob::new(p).unwrap());
        }
        b.build().unwrap()
    }

    #[test]
    fn basic_tag() {
        let html = build_img_tag("img.png", "a picture", "");
        assert_eq!(
            html,
            r#"<img src="img.png" alt="a picture" loading="lazy" decoding="async" />"#
        );
    }

    #[test]
    fn with_title() {
        let html = build_img_tag("img.png", "alt", "the title");
        assert_eq!(
            html,
            r#"<img src="img.png" alt="alt" title="the title" loading="lazy" decoding="async" />"#
        );
    }

    #[test]
    fn escapes_alt_html_specials() {
        // Attempted HTML injection in alt—must come out escaped.
        let html = build_img_tag("img.png", "a\"b<c>d&e", "");
        assert!(html.contains("alt=\"a&quot;b&lt;c&gt;d&amp;e\""));
    }

    #[test]
    fn escapes_url_ampersand() {
        let html = build_img_tag("img.png?a=1&b=2", "alt", "");
        // pulldown-cmark-escape writes `&` as `&amp;` in hrefs.
        assert!(html.contains("src=\"img.png?a=1&amp;b=2\""));
    }

    #[test]
    fn empty_alt_still_valid() {
        let html = build_img_tag("img.png", "", "");
        assert_eq!(
            html,
            r#"<img src="img.png" alt="" loading="lazy" decoding="async" />"#
        );
    }

    #[test]
    fn title_skipped_when_empty() {
        let html = build_img_tag("img.png", "alt", "");
        assert!(!html.contains("title="));
    }

    #[test]
    fn add_lazy_loading_collapses_image_events_into_html() {
        // Start(Image) + Text("alt") + End(Image) → single Html event with loading=
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("photo.jpg"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("alt text")),
            Event::End(TagEnd::Image),
        ];
        let out = add_lazy_loading(events);
        assert_eq!(out.len(), 1, "should collapse to one event");
        match &out[0] {
            Event::InlineHtml(html) => {
                assert!(
                    html.contains("loading="),
                    "missing loading attribute: {html}"
                );
                assert!(html.contains("alt=\"alt text\""), "missing alt: {html}");
                assert!(html.contains("src=\"photo.jpg\""), "missing src: {html}");
            }
            other => panic!("expected Html event, got {other:?}"),
        }
    }

    #[test]
    fn filter_by_hosts_drops_disallowed_image_to_alt_text() {
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://evil.com/bad.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("fallback alt")),
            Event::End(TagEnd::Image),
        ];
        let out = filter_by_hosts(events, &host_set(&["example.net"]));
        assert_eq!(out.len(), 1);
        match &out[0] {
            Event::Text(t) => assert_eq!(t.as_ref(), "fallback alt"),
            other => panic!("expected Text event, got {other:?}"),
        }
    }

    fn has_image_event(events: &[Event<'_>]) -> bool {
        events.iter().any(|e| {
            matches!(
                e,
                Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image)
            )
        })
    }

    #[test]
    fn image_nested_in_alt_text_is_consumed_with_its_parent() {
        let source = "![foo ![bar](https://evil.com/1)](https://evil.com/2)";
        let events: Vec<Event> = pulldown_cmark::Parser::new(source).collect();

        let dropped = filter_by_hosts(events.clone(), &host_set(&["example.net"]));
        assert!(!has_image_event(&dropped), "stray image event: {dropped:?}");
        assert!(dropped.contains(&Event::Text(CowStr::Borrowed("foo bar"))));

        let lazy = add_lazy_loading(events);
        assert!(!has_image_event(&lazy), "stray image event: {lazy:?}");
        assert!(lazy
            .iter()
            .any(|e| matches!(e, Event::InlineHtml(h) if h.contains("alt=\"foo bar\""))));
    }

    #[test]
    fn filter_by_hosts_drops_disallowed_image_with_empty_alt_entirely() {
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://evil.com/x.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::End(TagEnd::Image),
        ];
        let out = filter_by_hosts(events, &host_set(&["example.net"]));
        assert!(out.is_empty(), "expected zero events, got {out:?}");
    }

    #[test]
    fn filter_by_hosts_keeps_allowed_images_untouched() {
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://cdn.example.net/ok.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("alt")),
            Event::End(TagEnd::Image),
        ];
        let out = filter_by_hosts(events, &host_set(&["*.example.net"]));
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Start(Tag::Image { .. })));
    }

    #[test]
    fn filter_by_hosts_leaves_relative_images_alone() {
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("/local/pic.png"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("alt")),
            Event::End(TagEnd::Image),
        ];
        let out = filter_by_hosts(events, &host_set(&[]));
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Start(Tag::Image { .. })));
    }
}
