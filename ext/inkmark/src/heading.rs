//! Heading ID generation filter.
//!
//! When enabled, walks the event stream, collects the text content of each
//! heading that doesn't already have an id, and rewrites the `Event::Start`
//! to carry an auto-generated `id` derived from the heading text. Headings
//! that already have an id (via `heading_attributes: true`) are left alone.
//!
//! Duplicate base slugs get a counter suffix: `intro`, `intro-1`, `intro-2`.

use std::collections::HashMap;

use deunicode::deunicode_char;
use pulldown_cmark::{CowStr, Event, Tag, TagEnd};

/// Encapsulates slug deduplication logic: first use of a base slug is bare,
/// subsequent collisions get a `-N` suffix (intro, intro-1, intro-2, …).
///
/// Shared between `heading::add_ids` and `stats::collect` so both produce
/// identical slug sequences from the same heading stream.
pub struct SlugDeduplicator {
    seen: HashMap<String, usize>,
}

impl SlugDeduplicator {
    pub fn new() -> Self {
        Self {
            seen: HashMap::new(),
        }
    }

    /// Return the deduplicated slug for `base`. If `base` is empty it is
    /// returned as-is (the caller should skip it). Otherwise the first call
    /// with a given base returns the base unchanged; subsequent calls append
    /// `-1`, `-2`, etc.
    pub fn deduplicate(&mut self, base: String) -> String {
        if base.is_empty() {
            return base;
        }
        let count = self.seen.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        slug
    }
}

/// Apply heading-id generation to a full event stream in place.
///
/// Nested headings aren't possible in CommonMark so a single-level scan is
/// sufficient.
pub fn add_ids(events: &mut Vec<Event<'_>>) {
    let mut dedup = SlugDeduplicator::new();

    for i in 0..events.len() {
        // Only act on `Start(Heading)` events that lack an id.
        let needs_id = matches!(&events[i], Event::Start(Tag::Heading { id: None, .. }));
        if !needs_id {
            continue;
        }

        // Collect the raw text of this heading by scanning forward until
        // the matching `End(Heading)`.
        let text = collect_heading_text(events, i);
        let base = slugify(&text);
        if base.is_empty() {
            continue;
        }

        let slug = dedup.deduplicate(base);

        // Rebuild the heading event with the generated id.
        let placeholder = Event::SoftBreak;
        let old = std::mem::replace(&mut events[i], placeholder);
        if let Event::Start(Tag::Heading {
            level,
            classes,
            attrs,
            ..
        }) = old
        {
            events[i] = Event::Start(Tag::Heading {
                level,
                id: Some(CowStr::Boxed(slug.into_boxed_str())),
                classes,
                attrs,
            });
        }
    }
}

/// Walk forward from a `Start(Heading)` at index `start`, concatenating all
/// `Event::Text` and `Event::Code` payloads until the matching `End(Heading)`.
fn collect_heading_text(events: &[Event<'_>], start: usize) -> String {
    let mut text = String::new();
    let mut i = start + 1;
    while i < events.len() {
        match &events[i] {
            Event::End(TagEnd::Heading(_)) => return text,
            Event::Text(t) | Event::Code(t) => text.push_str(t),
            _ => {}
        }
        i += 1;
    }
    text
}

/// Convert heading text into a URL-safe slug for use as an `id` attribute.
///
/// Algorithm: walk the input char by char. ASCII alphanumerics are emitted
/// lowercased on a fast path without any transliteration lookup. Every
/// other character goes through `deunicode_char`, which returns an ASCII
/// transliteration. The ASCII expansion is then scanned the same way
/// as the input: alphanumerics pushed, anything else coalesced into a
/// single `-` separator with the usual no-double-dash collapse.
///
/// Leading separators never appear because we start with `prev_was_sep = true`;
/// trailing separators are stripped at the end. A heading whose entire
/// transliteration is empty produces an empty slug, so no id is emitted.
pub fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut prev_was_sep = true;

    for ch in text.chars() {
        // Fast path: ASCII alphanumeric
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_was_sep = false;
            continue;
        }

        match deunicode_char(ch) {
            Some(s) => {
                for r in s.chars() {
                    if r.is_ascii_alphanumeric() {
                        slug.push(r.to_ascii_lowercase());
                        prev_was_sep = false;
                    } else if !prev_was_sep {
                        slug.push('-');
                        prev_was_sep = true;
                    }
                }
            }
            None => {
                // Character has no known transliteration. Treat as a
                // separator boundary.
                if !prev_was_sep {
                    slug.push('-');
                    prev_was_sep = true;
                }
            }
        }
    }

    if slug.ends_with('-') {
        slug.pop();
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::{add_ids, slugify};
    use pulldown_cmark::{CowStr, Event, HeadingLevel, Tag, TagEnd};

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
    }

    #[test]
    fn slugify_trims_edges() {
        assert_eq!(slugify("  Leading and trailing  "), "leading-and-trailing");
    }

    #[test]
    fn slugify_collapses_runs() {
        assert_eq!(slugify("Spaces   between  words"), "spaces-between-words");
        assert_eq!(slugify("Multiple---Dashes"), "multiple-dashes");
    }

    #[test]
    fn slugify_plain_word() {
        assert_eq!(slugify("Introduction"), "introduction");
    }

    #[test]
    fn slugify_transliterates_latin_diacritics() {
        assert_eq!(slugify("Résumé"), "resume");
        assert_eq!(slugify("naïve"), "naive");
    }

    #[test]
    fn slugify_transliterates_cyrillic() {
        assert_eq!(slugify("Лев Толстой"), "lev-tolstoi");
        assert_eq!(slugify("Санкт-Петербург"), "sankt-peterburg");
    }

    #[test]
    fn slugify_transliterates_cjk() {
        assert_eq!(slugify("中文"), "zhong-wen");
        assert_eq!(slugify("Hello 中文 World"), "hello-zhong-wen-world");
    }

    #[test]
    fn add_ids_assigns_id_to_heading_without_one() {
        // Build: Start(Heading{id: None}) + Text("Hello") + End(Heading)
        let mut events = vec![
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                id: None,
                classes: vec![],
                attrs: vec![],
            }),
            Event::Text(CowStr::Borrowed("Hello")),
            Event::End(TagEnd::Heading(HeadingLevel::H1)),
        ];
        add_ids(&mut events);
        match &events[0] {
            Event::Start(Tag::Heading { id: Some(id), .. }) => {
                assert_eq!(id.as_ref(), "hello");
            }
            other => panic!("expected Start(Heading{{id: Some(_)}}), got {other:?}"),
        }
    }

    #[test]
    fn add_ids_deduplicates_colliding_slugs() {
        fn heading(text: &'static str) -> Vec<Event<'static>> {
            vec![
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H2,
                    id: None,
                    classes: vec![],
                    attrs: vec![],
                }),
                Event::Text(CowStr::Borrowed(text)),
                Event::End(TagEnd::Heading(HeadingLevel::H2)),
            ]
        }

        let mut events: Vec<Event> = heading("Intro")
            .into_iter()
            .chain(heading("Intro"))
            .chain(heading("Intro"))
            .collect();

        add_ids(&mut events);

        let ids: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                Event::Start(Tag::Heading { id: Some(id), .. }) => Some(id.to_string()),
                _ => None,
            })
            .collect();

        assert_eq!(ids, vec!["intro", "intro-1", "intro-2"]);
    }
}
