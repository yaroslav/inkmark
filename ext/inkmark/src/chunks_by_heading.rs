//! Heading-based section extraction for LLM / RAG pipelines.
//!
//! Splits a document into hierarchical sections by heading.
//! Each section's `content` is filter-applied Markdown: emoji
//! expanded, URLs autolinked, host/scheme allowlists applied,
//! then serialized back through `pulldown-cmark-to-cmark`.
//!
//! Designed as a first-stage chunking primitive for
//! `chunk → embed → retrieve` pipelines: feed a document in, get an
//! ordered array of heading-led sections out. The Ruby side wraps this
//! with an optional `heading:` filter (String or Regexp).
//!
//! Heading Start/End pairs survive the filter pipeline intact —
//! emoji/autolink rewrites happen *inside* a heading's text events,
//! but the bracketing tags stay in place—so post-filter heading-
//! position scanning is coherent.

use magnus::{Error, RArray, RHash, Ruby};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{apply_filters, content_events};
use crate::heading::{self, SlugDeduplicator};
use crate::options::build_options;
use crate::toc;
use crate::truncate::{self, TruncateParams};

pub fn native_chunks_by_heading(
    ruby: &Ruby,
    source: String,
    opts_hash: RHash,
) -> Result<RArray, Error> {
    // The Ruby side merges the optional `truncate:` kwarg into the
    // opts hash under the `:truncate` key before calling us. We pull
    // it out here; the rest of `build_options` ignores it.
    let truncate_params: Option<TruncateParams> = {
        let nested: Option<RHash> = opts_hash.lookup(ruby.to_symbol("truncate"))?;
        match nested {
            Some(h) => Some(truncate::parse_params(ruby, h)?),
            None => None,
        }
    };
    let (cm_opts, flags) = build_options(ruby, opts_hash)?;

    // Parse + run the full filter pipeline, same as `to_markdown`.
    // `content_events` drops frontmatter so it never becomes a section.
    let events: Vec<Event> = content_events(&source, cm_opts).collect();
    let events = apply_filters(events, &flags);

    let boundaries = find_heading_boundaries(&events);
    let result = ruby.ary_new();

    // Preamble: events before the first heading, or the whole doc
    // when there are no headings at all. Emitted as an entry with
    // `heading: nil, level: 0, id: nil`. Skipped entirely when there
    // is no non-empty content before the first heading.
    let with_counts = flags.statistics;
    let preamble_end = boundaries.first().map(|b| b.start).unwrap_or(events.len());
    if preamble_end > 0 {
        let preamble_events = &events[0..preamble_end];
        if !is_empty_content(preamble_events) {
            result.push(build_preamble_hash(
                ruby,
                preamble_events,
                with_counts,
                truncate_params.as_ref(),
            )?)?;
        }
    }

    // One entry per heading. Section end is the position of the
    // next heading with level <= current level (or end of events).
    //
    // `ancestors` tracks the heading stack so we can attach a
    // breadcrumb (root → immediate-parent) to each section. At
    // each boundary we pop any ancestors whose level is >= the
    // current boundary's level (those aren't parents), then record
    // the remaining stack as this section's breadcrumb, then push
    // the current heading for its own subsections' use.
    let mut dedup = SlugDeduplicator::new();
    let mut ancestors: Vec<(u8, String)> = Vec::new();
    for (i, boundary) in boundaries.iter().enumerate() {
        let section_end = find_section_end(&boundaries, i, events.len());
        let level = toc::level_to_u8(boundary.level);
        while ancestors.last().is_some_and(|(l, _)| *l >= level) {
            ancestors.pop();
        }
        let breadcrumb: Vec<&str> = ancestors.iter().map(|(_, t)| t.as_str()).collect();
        let heading_text = collect_inline_text(&events[(boundary.start + 1)..boundary.end]);
        let hash = build_section_hash(
            ruby,
            &events,
            boundary,
            section_end,
            &heading_text,
            &breadcrumb,
            &mut dedup,
            with_counts,
            truncate_params.as_ref(),
        )?;
        result.push(hash)?;
        ancestors.push((level, heading_text));
    }

    Ok(result)
}

/// A discovered `Start(Heading) / End(Heading)` pair in the filtered
/// event stream.
struct HeadingBoundary {
    start: usize,
    end: usize,
    level: HeadingLevel,
}

fn find_heading_boundaries(events: &[Event<'_>]) -> Vec<HeadingBoundary> {
    let mut boundaries = Vec::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let lvl = *level;
            // CommonMark disallows headings inside headings, but carry a
            // depth counter so we stay correct if pulldown-cmark ever
            // permits them.
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < events.len() {
                match &events[j] {
                    Event::Start(Tag::Heading { .. }) => depth += 1,
                    Event::End(TagEnd::Heading(_)) => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            boundaries.push(HeadingBoundary {
                start: i,
                end: j,
                level: lvl,
            });
            i = j + 1;
        } else {
            i += 1;
        }
    }
    boundaries
}

/// Section i ends at the first subsequent heading with level <=
/// current. Headings with a strictly greater level are subsections:
/// they belong to the current section's content too.
fn find_section_end(boundaries: &[HeadingBoundary], i: usize, events_len: usize) -> usize {
    let current = toc::level_to_u8(boundaries[i].level);
    boundaries[(i + 1)..]
        .iter()
        .find(|b| toc::level_to_u8(b.level) <= current)
        .map(|b| b.start)
        .unwrap_or(events_len)
}

fn build_preamble_hash(
    ruby: &Ruby,
    events: &[Event<'_>],
    with_counts: bool,
    truncate_params: Option<&TruncateParams>,
) -> Result<RHash, Error> {
    let hash = ruby.hash_new();
    hash.aset(ruby.to_symbol("heading"), ())?;
    hash.aset(ruby.to_symbol("level"), 0u8)?;
    hash.aset(ruby.to_symbol("id"), ())?;
    // Preamble has no ancestors; empty array keeps the shape uniform
    // with proper sections so callers can treat every entry alike.
    hash.aset(ruby.to_symbol("breadcrumb"), ruby.ary_new_capa(0))?;

    let content = match truncate_params {
        Some(params) => truncate::truncate_events(events, params),
        None => render_markdown(events),
    };
    if with_counts {
        let (chars, words) = count_post_truncate(events, truncate_params, &content);
        hash.aset(ruby.to_symbol("character_count"), chars)?;
        hash.aset(ruby.to_symbol("word_count"), words)?;
    }
    hash.aset(ruby.to_symbol("content"), content)?;
    Ok(hash)
}

fn build_section_hash(
    ruby: &Ruby,
    events: &[Event<'_>],
    boundary: &HeadingBoundary,
    section_end: usize,
    heading_text: &str,
    breadcrumb: &[&str],
    dedup: &mut SlugDeduplicator,
    with_counts: bool,
    truncate_params: Option<&TruncateParams>,
) -> Result<RHash, Error> {
    // Slug is the deduplicated slugify of the (filter-applied) heading
    // text, matching the ids `heading_ids` / `toc` would emit for the
    // same document.
    let base = heading::slugify(heading_text);
    let id = if base.is_empty() {
        String::new()
    } else {
        dedup.deduplicate(base)
    };

    // Content = events after End(Heading) up to the next section or
    // end of document. Re-serialized through cmark_write.
    let content_events = &events[(boundary.end + 1)..section_end];

    let hash = ruby.hash_new();
    hash.aset(ruby.to_symbol("heading"), heading_text)?;
    hash.aset(ruby.to_symbol("level"), toc::level_to_u8(boundary.level))?;
    if id.is_empty() {
        hash.aset(ruby.to_symbol("id"), ())?;
    } else {
        hash.aset(ruby.to_symbol("id"), id)?;
    }
    let breadcrumb_arr = ruby.ary_new_capa(breadcrumb.len());
    for text in breadcrumb {
        breadcrumb_arr.push(*text)?;
    }
    hash.aset(ruby.to_symbol("breadcrumb"), breadcrumb_arr)?;

    let content = match truncate_params {
        Some(params) => truncate::truncate_events(content_events, params),
        None => render_markdown(content_events),
    };
    if with_counts {
        let (chars, words) = count_post_truncate(content_events, truncate_params, &content);
        hash.aset(ruby.to_symbol("character_count"), chars)?;
        hash.aset(ruby.to_symbol("word_count"), words)?;
    }
    hash.aset(ruby.to_symbol("content"), content)?;
    Ok(hash)
}

/// Return (character_count, word_count) for a section.
///
/// Without truncation: counts come from the original event stream's
/// Text/Code events.
///
/// With truncation: reparse the truncated Markdown and count from its
/// events.
fn count_post_truncate(
    original_events: &[Event<'_>],
    truncate_params: Option<&TruncateParams>,
    truncated_content: &str,
) -> (usize, usize) {
    if truncate_params.is_none() {
        return count_text(original_events);
    }
    let events: Vec<Event> = Parser::new_ext(
        truncated_content,
        pulldown_cmark::Options::ENABLE_GFM
            | pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_TASKLISTS
            | pulldown_cmark::Options::ENABLE_FOOTNOTES,
    )
    .collect();
    count_text(&events)
}

/// Count characters (after trimming) and unicode words in a section's
/// Text/Code event stream. Code-block contents are included: matches
/// document-level {stats::collect} semantics and reflects what an
/// embedding model would actually consume.
fn count_text(events: &[Event<'_>]) -> (usize, usize) {
    let mut buf = String::new();
    for event in events {
        match event {
            Event::Text(t) | Event::Code(t) => {
                buf.push_str(t);
                buf.push(' ');
            }
            Event::SoftBreak | Event::HardBreak => buf.push(' '),
            _ => {}
        }
    }
    let chars = buf.trim().chars().count();
    let words = buf.unicode_words().count();
    (chars, words)
}

fn render_markdown(events: &[Event<'_>]) -> String {
    let mut buf = String::new();
    pulldown_cmark_to_cmark::cmark(events.iter().cloned(), &mut buf)
        .expect("markdown serialization failed");
    buf
}

fn collect_inline_text(events: &[Event<'_>]) -> String {
    let mut out = String::new();
    for event in events {
        match event {
            Event::Text(t) | Event::Code(t) => out.push_str(t),
            _ => {}
        }
    }
    out
}

/// A preamble (or whole-doc when there are no headings) is meaningful
/// only when it contains actual content: text, code, or raw HTML.
/// Whitespace-only event streams produce an empty preamble entry that
/// would just add noise.
fn is_empty_content(events: &[Event<'_>]) -> bool {
    !events.iter().any(|e| {
        matches!(
            e,
            Event::Text(_)
                | Event::Code(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_)
        )
    })
}
