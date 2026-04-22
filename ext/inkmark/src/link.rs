//! External link `rel` attribute injection filter and URL matcher.
//!
//! When enabled, replaces the `Start(Tag::Link)` and matching
//! `End(TagEnd::Link)` events for every external link with hand-built
//! `<a href="..." rel="nofollow noopener">` / `</a>` HTML events. Inner
//! events (text, emphasis, inline code, images) pass through unchanged,
//! so pulldown-cmark's built-in writers still render the link content:
//! we only replace the opening and closing tags.
//!
//! "External" here means the URL starts with `http://` or `https://`
//! (case-insensitive). Relative paths, anchor fragments, and non-web
//! schemes (`mailto:`, `tel:`, `javascript:`) are not touched:

use globset::GlobSet;
use pulldown_cmark::{CowStr, Event, Tag, TagEnd};
use pulldown_cmark_escape::{escape_href, escape_html};

use crate::url_match::is_host_allowed;

/// Add `rel="nofollow noopener"` to every external `<a>` tag by replacing
/// its `Start(Link)` event with a synthesized `Event::Html` opening tag
/// and its matching `End(Link)` event with a `</a>` close tag.
pub fn add_nofollow(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut out: Vec<Event<'_>> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Link {
                link_type: _,
                ref dest_url,
                ref title,
                id: _,
            }) if is_external(dest_url) => {
                let open = build_link_open(dest_url, title);
                out.push(Event::Html(CowStr::Boxed(open.into_boxed_str())));

                // Consume inner events through the matching End(Link),
                // depth-counting so a nested link doesn't break the
                // close-tag pairing. CommonMark disallows nested links
                // in valid markdown, so depth should always reach zero on
                // the first End we see.
                let mut depth: usize = 1;
                for inner in iter.by_ref() {
                    let is_link_start = matches!(&inner, Event::Start(Tag::Link { .. }));
                    let is_link_end = matches!(&inner, Event::End(TagEnd::Link));

                    if is_link_start {
                        depth += 1;
                        out.push(inner);
                    } else if is_link_end {
                        depth -= 1;
                        if depth == 0 {
                            out.push(Event::Html(CowStr::Borrowed("</a>")));
                            break;
                        }
                        out.push(inner);
                    } else {
                        out.push(inner);
                    }
                }
            }
            other => out.push(other),
        }
    }

    out
}

/// Drop `<a>` tags whose destination URL's host isn't in the allowlist,
/// leaving the inner content (text, emphasis, images) in place as a
/// bare phrase. Non-web URLs (relative paths, `mailto:`, etc.) pass
/// through.
pub fn filter_by_hosts<'a>(events: Vec<Event<'a>>, set: &GlobSet) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();

    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Link { ref dest_url, .. }) if !is_host_allowed(dest_url, set) => {
                let mut depth: usize = 1;
                for inner in iter.by_ref() {
                    match &inner {
                        Event::Start(Tag::Link { .. }) => {
                            depth += 1;
                            out.push(inner);
                        }
                        Event::End(TagEnd::Link) => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            out.push(inner);
                        }
                        _ => out.push(inner),
                    }
                }
            }
            other => out.push(other),
        }
    }

    out
}

/// Return true when the URL starts with `http://` or `https://` (case
/// insensitive). Relative paths, anchor fragments, and `mailto:` /
/// `tel:` / `javascript:` URLs return false.
#[inline]
fn is_external(url: &str) -> bool {
    url.split_once("://").is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    })
}

/// Construct the `<a href="..." title="..." rel="nofollow noopener">`
/// opening tag. The URL goes through `escape_href` (percent-encoding +
/// HTML-special escaping, matching pulldown-cmark's upstream behavior),
/// and the title through `escape_html` for attribute context.
#[inline]
fn build_link_open(href: &str, title: &str) -> String {
    let mut out = String::with_capacity(40 + href.len() + title.len());
    out.push_str("<a href=\"");
    let _ = escape_href(&mut out, href);
    out.push('"');
    if !title.is_empty() {
        out.push_str(" title=\"");
        let _ = escape_html(&mut out, title);
        out.push('"');
    }
    out.push_str(" rel=\"nofollow noopener\">");
    out
}

#[cfg(test)]
mod tests {
    use super::{add_nofollow, build_link_open, filter_by_hosts, is_external};
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
    fn external_detection() {
        assert!(is_external("http://example.net"));
        assert!(is_external("https://example.net"));
        assert!(is_external("HTTPS://EXAMPLE.NET"));
        assert!(is_external("Http://mixed.case"));

        assert!(!is_external("/local/path"));
        assert!(!is_external("relative.html"));
        assert!(!is_external("#anchor"));
        assert!(!is_external("mailto:user@example.net"));
        assert!(!is_external("tel:+1234567890"));
        assert!(!is_external("javascript:alert(1)"));
        assert!(!is_external("//protocol-relative.com"));
        assert!(!is_external(""));
        assert!(!is_external("h"));
        assert!(!is_external("http"));
        assert!(!is_external("https"));
    }

    #[test]
    fn open_tag_basic() {
        assert_eq!(
            build_link_open("https://example.net", ""),
            r#"<a href="https://example.net" rel="nofollow noopener">"#
        );
    }

    #[test]
    fn open_tag_with_title() {
        assert_eq!(
            build_link_open("https://example.net", "the title"),
            r#"<a href="https://example.net" title="the title" rel="nofollow noopener">"#
        );
    }

    #[test]
    fn open_tag_escapes_url_ampersand() {
        let tag = build_link_open("https://example.net/?a=1&b=2", "");
        assert!(tag.contains(r#"href="https://example.net/?a=1&amp;b=2""#));
    }

    #[test]
    fn open_tag_escapes_title_specials() {
        let tag = build_link_open("https://example.net", r#"a "quoted" <title>"#);
        assert!(tag.contains("&quot;quoted&quot;"));
        assert!(tag.contains("&lt;title&gt;"));
    }

    #[test]
    fn add_nofollow_adds_rel_to_external_link() {
        // Start(Link) + Text("click") + End(Link) → Html open + Text + Html close
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://example.net"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("click")),
            Event::End(TagEnd::Link),
        ];
        let out = add_nofollow(events);
        // Should produce: Html(open), Text("click"), Html("</a>")
        assert_eq!(out.len(), 3, "expected 3 events, got {}", out.len());
        match &out[0] {
            Event::Html(html) => {
                assert!(
                    html.contains("nofollow"),
                    "opening tag must contain nofollow: {html}"
                );
                assert!(
                    html.contains("https://example.net"),
                    "opening tag must contain href: {html}"
                );
            }
            other => panic!("expected Html open event, got {other:?}"),
        }
        match &out[2] {
            Event::Html(html) => assert_eq!(html.as_ref(), "</a>"),
            other => panic!("expected Html close event, got {other:?}"),
        }
    }

    #[test]
    fn filter_by_hosts_drops_disallowed_link_tags_keeping_text() {
        // Start(Link to evil) + Text("click") + End(Link) →
        // just Text("click"), with the link wrapper gone.
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://evil.com"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("click me")),
            Event::End(TagEnd::Link),
        ];
        let out = filter_by_hosts(events, &host_set(&["example.net"]));
        assert_eq!(out.len(), 1);
        match &out[0] {
            Event::Text(t) => assert_eq!(t.as_ref(), "click me"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn filter_by_hosts_keeps_allowed_links_untouched() {
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://cdn.example.net/doc"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("ok")),
            Event::End(TagEnd::Link),
        ];
        let out = filter_by_hosts(events, &host_set(&["*.example.net"]));
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Start(Tag::Link { .. })));
        assert!(matches!(out[2], Event::End(TagEnd::Link)));
    }

    #[test]
    fn filter_by_hosts_leaves_relative_and_mailto_alone() {
        // Even with an empty allowlist that blocks everything external,
        // relative/mailto links pass through unchanged.
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("/local"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("home")),
            Event::End(TagEnd::Link),
        ];
        let out = filter_by_hosts(events, &host_set(&[]));
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Start(Tag::Link { .. })));
    }
}
