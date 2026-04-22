//! Streaming scheme-allowlist filter over pulldown-cmark events.
//!
//! Wraps any `Iterator<Item = Event<'a>>` and filters out markdown-emitted
//! Link/Image events whose URL scheme is not in the respective allowlist.
//!
//! This replaces:
//!
//! - Blocked link: `Start(Link)` and matching `End(Link)` both dropped;
//!   inner events (text, emphasis, nested markup) pass through as bare
//!   content. Defensive depth counting handles malformed nested links.
//! - Blocked image: the entire `Start(Image) ... End(Image)` sequence is
//!   replaced with a single `Event::Text` carrying the accumulated alt
//!   text, or nothing if alt was empty.
//! - `Option<&[String]>` is `None` for "don't filter this kind" (caller
//!   opted out). Empty slice `Some(&[])` blocks every absolute URL.

use pulldown_cmark::{CowStr, Event, Tag, TagEnd};

use crate::url_match::is_scheme_allowed;

/// Streaming adapter that drops Link/Image events with disallowed schemes.
///
/// `'e` is the lifetime of the `Event` borrows from the underlying parser,
/// `'s` is the lifetime of the scheme-allowlist slices.
/// Separate lifetimes matter because in the fast path the slices come
/// from `&'static` `OnceLock` storage (outlives everything), while events
/// are tied to the source string—tying them into one lifetime would
/// constrain callers unnecessarily.
pub struct SchemeFilter<'e, 's, I: Iterator<Item = Event<'e>>> {
    inner: I,
    link_allowed: Option<&'s [String]>,
    image_allowed: Option<&'s [String]>,
    // State for in-progress link drop. 0 = not skipping; N>0 = inside a
    // blocked link with N nested Link starts yet to close. CommonMark
    // disallows nested links so this is usually 1, but the counter keeps
    // us correct on malformed / extension-emitted streams.
    skipping_link_depth: usize,
    // State for in-progress image drop.
    skipping_image: bool,
    image_alt: String,
}

impl<'e, 's, I: Iterator<Item = Event<'e>>> SchemeFilter<'e, 's, I> {
    pub fn new(
        inner: I,
        link_allowed: Option<&'s [String]>,
        image_allowed: Option<&'s [String]>,
    ) -> Self {
        Self {
            inner,
            link_allowed,
            image_allowed,
            skipping_link_depth: 0,
            skipping_image: false,
            image_alt: String::new(),
        }
    }
}

impl<'e, 's, I: Iterator<Item = Event<'e>>> Iterator for SchemeFilter<'e, 's, I> {
    type Item = Event<'e>;

    fn next(&mut self) -> Option<Event<'e>> {
        loop {
            // Image skipping mode
            if self.skipping_image {
                match self.inner.next()? {
                    Event::End(TagEnd::Image) => {
                        self.skipping_image = false;
                        if !self.image_alt.is_empty() {
                            let alt = std::mem::take(&mut self.image_alt);
                            return Some(Event::Text(CowStr::Boxed(alt.into_boxed_str())));
                        }
                        continue;
                    }
                    Event::Text(t) | Event::Code(t) => {
                        self.image_alt.push_str(&t);
                        continue;
                    }
                    _ => continue,
                }
            }

            // Link skipping mode
            if self.skipping_link_depth > 0 {
                let ev = self.inner.next()?;
                match &ev {
                    Event::Start(Tag::Link { .. }) => {
                        self.skipping_link_depth += 1;
                        return Some(ev);
                    }
                    Event::End(TagEnd::Link) => {
                        self.skipping_link_depth -= 1;
                        if self.skipping_link_depth == 0 {
                            continue;
                        } else {
                            return Some(ev);
                        }
                    }
                    _ => return Some(ev),
                }
            }

            // Check each event for a drop trigger
            let ev = self.inner.next()?;
            match &ev {
                Event::Start(Tag::Link { dest_url, .. })
                    if self
                        .link_allowed
                        .is_some_and(|s| !is_scheme_allowed(dest_url, s)) =>
                {
                    self.skipping_link_depth = 1;
                    continue;
                }
                Event::Start(Tag::Image { dest_url, .. })
                    if self
                        .image_allowed
                        .is_some_and(|s| !is_scheme_allowed(dest_url, s)) =>
                {
                    self.skipping_image = true;
                    self.image_alt.clear();
                    continue;
                }
                _ => return Some(ev),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SchemeFilter;
    use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

    fn schemes(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_ascii_lowercase()).collect()
    }

    fn run<'a>(
        events: Vec<Event<'a>>,
        link: Option<&'a [String]>,
        image: Option<&'a [String]>,
    ) -> Vec<Event<'a>> {
        SchemeFilter::new(events.into_iter(), link, image).collect()
    }

    #[test]
    fn passes_allowed_link_through() {
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("https://example.net"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("t")),
            Event::End(TagEnd::Link),
        ];
        let allowed = schemes(&["https"]);
        let out = run(events, Some(&allowed), None);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Start(Tag::Link { .. })));
    }

    #[test]
    fn drops_blocked_link_keeps_text() {
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("javascript:alert(1)"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("click")),
            Event::End(TagEnd::Link),
        ];
        let allowed = schemes(&["http", "https"]);
        let out = run(events, Some(&allowed), None);
        assert_eq!(out.len(), 1);
        match &out[0] {
            Event::Text(t) => assert_eq!(t.as_ref(), "click"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn drops_blocked_image_to_alt() {
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("data:image/svg+xml,<svg/>"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("fallback")),
            Event::End(TagEnd::Image),
        ];
        let allowed = schemes(&["http", "https"]);
        let out = run(events, None, Some(&allowed));
        assert_eq!(out.len(), 1);
        match &out[0] {
            Event::Text(t) => assert_eq!(t.as_ref(), "fallback"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn drops_blocked_image_empty_alt_entirely() {
        let events = vec![
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("data:image/svg+xml,<svg/>"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::End(TagEnd::Image),
        ];
        let allowed = schemes(&["https"]);
        let out = run(events, None, Some(&allowed));
        assert!(out.is_empty());
    }

    #[test]
    fn fuses_link_and_image_filters_in_one_pass() {
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("javascript:alert(1)"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("bad link")),
            Event::End(TagEnd::Link),
            Event::Text(CowStr::Borrowed(" and ")),
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("data:x"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("bad pic")),
            Event::End(TagEnd::Image),
        ];
        let link = schemes(&["https"]);
        let image = schemes(&["https"]);
        let out = run(events, Some(&link), Some(&image));
        // Expect: Text("bad link"), Text(" and "), Text("bad pic")
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Text(_)));
        assert!(matches!(out[1], Event::Text(_)));
        assert!(matches!(out[2], Event::Text(_)));
    }

    #[test]
    fn relative_urls_pass_through() {
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
        // Even with an empty allowlist that blocks all absolute URLs,
        // relative URLs pass through.
        let allowed = schemes(&[]);
        let out = run(events, Some(&allowed), None);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Start(Tag::Link { .. })));
    }

    #[test]
    fn none_allowlist_means_no_filter() {
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("javascript:alert(1)"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("click")),
            Event::End(TagEnd::Link),
        ];
        let out = run(events, None, None);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Event::Start(Tag::Link { .. })));
    }

    #[test]
    fn independent_link_and_image_control() {
        // Link filter off, image filter on—only image should drop.
        let events = vec![
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("javascript:x"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("stays")),
            Event::End(TagEnd::Link),
            Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url: CowStr::Borrowed("data:x"),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(CowStr::Borrowed("dropped")),
            Event::End(TagEnd::Image),
        ];
        let image = schemes(&["https"]);
        let out = run(events, None, Some(&image));
        // Link passes (3 events), image drops to Text (1 event).
        assert_eq!(out.len(), 4);
        assert!(matches!(out[0], Event::Start(Tag::Link { .. })));
        assert!(matches!(out[3], Event::Text(_)));
    }
}
