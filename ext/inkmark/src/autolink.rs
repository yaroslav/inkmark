//! Auto-linking filter for bare URLs and email addresses.
//!
//! When enabled, scans `Event::Text` payloads for bare URLs and emails
//! using the `linkify` crate, and splits them into alternating
//! `Event::Text` / `Event::Start(Link)` + `Event::Text` + `Event::End(Link)`
//! sequences. Text inside code blocks and existing links is not touched.

use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

/// Scan text events for bare URLs/emails and wrap them in link events.
/// Tracks link and code-block depth so we don't autolink inside existing
/// links or code blocks.
pub fn autolink(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let finder = LinkFinder::new();
    let mut out: Vec<Event<'_>> = Vec::with_capacity(events.len());
    let mut link_depth: usize = 0;
    let mut code_depth: usize = 0;

    for event in events {
        match &event {
            Event::Start(Tag::Link { .. }) => link_depth += 1,
            Event::End(TagEnd::Link) => link_depth = link_depth.saturating_sub(1),
            Event::Start(Tag::CodeBlock(_)) => code_depth += 1,
            Event::End(TagEnd::CodeBlock) => code_depth = code_depth.saturating_sub(1),
            _ => {}
        }

        // Only process Text events outside links and code blocks.
        let dominated = link_depth > 0 || code_depth > 0;
        let is_text = matches!(&event, Event::Text(_));

        if !is_text || dominated {
            out.push(event);
            continue;
        }

        // Extract the text and scan for links.
        if let Event::Text(text) = event {
            let spans: Vec<_> = finder.spans(&text).collect();

            // Fast path: no links found—push original event unchanged.
            if spans.iter().all(|s| s.kind().is_none()) {
                out.push(Event::Text(text));
                continue;
            }

            // Split the text into alternating plain / link spans.
            for span in spans {
                let fragment = &text[span.start()..span.end()];
                match span.kind() {
                    Some(LinkKind::Url) => {
                        let url = CowStr::Boxed(fragment.to_string().into_boxed_str());
                        let display = CowStr::Boxed(fragment.to_string().into_boxed_str());
                        out.push(Event::Start(Tag::Link {
                            link_type: LinkType::Autolink,
                            dest_url: url,
                            title: CowStr::Borrowed(""),
                            id: CowStr::Borrowed(""),
                        }));
                        out.push(Event::Text(display));
                        out.push(Event::End(TagEnd::Link));
                    }
                    Some(LinkKind::Email) => {
                        // pulldown-cmark's HTML writer adds "mailto:" for
                        // LinkType::Email, so we pass just the address.
                        let addr = CowStr::Boxed(fragment.to_string().into_boxed_str());
                        let display = CowStr::Boxed(fragment.to_string().into_boxed_str());
                        out.push(Event::Start(Tag::Link {
                            link_type: LinkType::Email,
                            dest_url: addr,
                            title: CowStr::Borrowed(""),
                            id: CowStr::Borrowed(""),
                        }));
                        out.push(Event::Text(display));
                        out.push(Event::End(TagEnd::Link));
                    }
                    Some(_) | None => {
                        // Plain text segment—no link.
                        if !fragment.is_empty() {
                            out.push(Event::Text(CowStr::Boxed(
                                fragment.to_string().into_boxed_str(),
                            )));
                        }
                    }
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::autolink;
    use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

    #[test]
    fn bare_url_becomes_link() {
        let events = vec![Event::Text(CowStr::Borrowed(
            "Visit https://example.net today",
        ))];
        let out = autolink(events);
        // Should produce: Text("Visit ") + Start(Link) + Text("https://example.com") + End(Link) + Text(" today")
        assert!(out.len() >= 5, "expected split events, got {}", out.len());
        let has_link = out
            .iter()
            .any(|e| matches!(e, Event::Start(Tag::Link { .. })));
        assert!(has_link, "no link event found");
    }

    #[test]
    fn email_becomes_email_link() {
        let events = vec![Event::Text(CowStr::Borrowed("Contact user@example.com"))];
        let out = autolink(events);
        // pulldown-cmark's HTML writer adds "mailto:" for LinkType::Email,
        // so we only store the bare address in dest_url.
        let has_email = out.iter().any(|e| match e {
            Event::Start(Tag::Link {
                link_type: LinkType::Email,
                dest_url,
                ..
            }) => dest_url.as_ref() == "user@example.com",
            _ => false,
        });
        assert!(has_email, "no email link found in {out:?}");
    }

    #[test]
    fn text_without_urls_unchanged() {
        let events = vec![Event::Text(CowStr::Borrowed("just plain text"))];
        let out = autolink(events);
        assert_eq!(out.len(), 1);
        assert!(matches!(&out[0], Event::Text(t) if t.as_ref() == "just plain text"));
    }

    #[test]
    fn skips_inside_existing_links() {
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://example.net"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("https://example.net")),
            Event::End(TagEnd::Link),
        ];
        let out = autolink(events);
        // Should be unchanged—3 events, no extra links added.
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn skips_inside_code_blocks() {
        let events = vec![
            Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(
                CowStr::Borrowed(""),
            ))),
            Event::Text(CowStr::Borrowed("https://example.net")),
            Event::End(TagEnd::CodeBlock),
        ];
        let out = autolink(events);
        assert_eq!(out.len(), 3);
    }
}
