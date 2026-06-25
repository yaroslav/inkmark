//! Sliding-window chunking for LLM / RAG pipelines.
//!
//! Splits a document into fixed-size chunks with optional overlap.
//! Unlike `chunks_by_heading` (which uses document structure), this
//! walks the filter-applied Markdown sequentially and emits windows
//! bounded by a character and/or word budget.
//!
//! Two boundary modes:
//! - [`BoundaryAt::Block`]: cut only between top-level Markdown blocks.
//!   Output is always valid Markdown. Oversized blocks are emitted
//!   as their own windows (decision A).
//! - [`BoundaryAt::Word`]: serialize the full filtered Markdown, cut
//!   at the last Unicode word boundary that fits. Tighter fit but may
//!   split open constructs (code fences, links).
//!
//! Overlap is measured in chars. Each new window begins with the
//! trailing `overlap` chars of the previous window's content, so
//! adjacent chunks share context.

use magnus::{Error, RArray, RHash, Ruby};
use pulldown_cmark::Event;
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{apply_filters, content_events};
use crate::options::build_options;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BoundaryAt {
    Block,
    Word,
}

pub struct WindowParams {
    pub chars: Option<usize>,
    pub words: Option<usize>,
    pub overlap: usize,
    pub at: BoundaryAt,
}

pub fn native_chunks_by_size(
    ruby: &Ruby,
    source: String,
    opts_hash: RHash,
) -> Result<RArray, Error> {
    let params = parse_params(ruby, &opts_hash)?;
    let (cm_opts, flags) = build_options(ruby, opts_hash)?;

    let events: Vec<Event> = content_events(&source, cm_opts).collect();
    let events = apply_filters(events, &flags);

    let windows = match params.at {
        BoundaryAt::Block => chunk_blocks(&events, &params),
        BoundaryAt::Word => chunk_words(&events, &params),
    };

    build_result(ruby, &windows, flags.statistics)
}

fn parse_params(ruby: &Ruby, hash: &RHash) -> Result<WindowParams, Error> {
    let nested: Option<RHash> = hash.lookup(ruby.to_symbol("__window"))?;
    let params =
        nested.ok_or_else(|| Error::new(ruby.exception_arg_error(), "missing window params"))?;

    let chars: Option<usize> = params.lookup(ruby.to_symbol("chars"))?;
    let words: Option<usize> = params.lookup(ruby.to_symbol("words"))?;
    let overlap: Option<usize> = params.lookup(ruby.to_symbol("overlap"))?;
    let at_str: Option<String> = params.lookup(ruby.to_symbol("at"))?;

    let at = match at_str.as_deref() {
        Some("word") => BoundaryAt::Word,
        _ => BoundaryAt::Block,
    };

    Ok(WindowParams {
        chars,
        words,
        overlap: overlap.unwrap_or(0),
        at,
    })
}

fn chunk_blocks(events: &[Event<'_>], params: &WindowParams) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_chars: usize = 0;
    let mut current_words: usize = 0;

    for (start, end) in top_level_blocks(events) {
        let block = render_markdown(&events[start..=end]);
        let block_chars = block.chars().count();
        let block_words = block.unicode_words().count();

        let would_exceed_chars = params
            .chars
            .map(|b| current_chars + block_chars > b)
            .unwrap_or(false);
        let would_exceed_words = params
            .words
            .map(|b| current_words + block_words > b)
            .unwrap_or(false);

        // Oversized blocks (a single block larger than the
        // budget) get emitted as their own window, never silently
        // dropped or truncated.
        if (would_exceed_chars || would_exceed_words) && !current.is_empty() {
            let finished = std::mem::take(&mut current);
            current = seed_with_overlap(&finished, params.overlap);
            current_chars = current.chars().count();
            current_words = current.unicode_words().count();
            out.push(finished);
        }

        current.push_str(&block);
        current_chars += block_chars;
        current_words += block_words;
    }

    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Return the trailing `overlap` characters of `s`, aligned to a
/// char boundary. When `overlap == 0` or `s` is shorter than the
/// overlap budget, return an empty string.
fn seed_with_overlap(s: &str, overlap: usize) -> String {
    if overlap == 0 {
        return String::new();
    }
    let total = s.chars().count();
    if overlap >= total {
        return String::new();
    }
    let skip = total - overlap;
    s.chars().skip(skip).collect()
}

fn top_level_blocks(events: &[Event<'_>]) -> Vec<(usize, usize)> {
    let mut blocks = Vec::new();
    let mut depth: i32 = 0;
    let mut current_start: Option<usize> = None;

    for (i, event) in events.iter().enumerate() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    current_start = Some(i);
                }
                depth += 1;
            }
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = current_start.take() {
                        blocks.push((start, i));
                    }
                }
            }
            _ => {
                if depth == 0 {
                    blocks.push((i, i));
                }
            }
        }
    }
    blocks
}

fn render_markdown(events: &[Event<'_>]) -> String {
    let mut buf = String::new();
    pulldown_cmark_to_cmark::cmark(events.iter().cloned(), &mut buf)
        .expect("markdown serialization failed");
    buf
}

// Serialize the full filtered Markdown into one string, then walk
// word boundaries. Each window is a byte-aligned slice of the
// serialized output ending at a word boundary that fits the budget.
// Overlap is implemented by advancing the window start backward by
// `overlap` chars (to the next word boundary) before taking the
// next window.

fn chunk_words(events: &[Event<'_>], params: &WindowParams) -> Vec<String> {
    let rendered = render_markdown(events);
    if rendered.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::new();
    let mut cursor: usize = 0; // byte offset into `rendered`
    let bytes_len = rendered.len();

    while cursor < bytes_len {
        // Find the largest byte offset `end_byte` such that the chars
        // in rendered[cursor..end_byte] fit both char and word budgets
        // and end on a word boundary.
        let slice = &rendered[cursor..];
        let mut used_chars: usize = 0;
        let mut used_words: usize = 0;
        let mut last_good_byte: usize = 0;

        for (offset, segment) in slice.split_word_bound_indices() {
            let seg_chars = segment.chars().count();
            let seg_is_word = segment.unicode_words().next().is_some();

            let next_chars = used_chars + seg_chars;
            let next_words = if seg_is_word {
                used_words + 1
            } else {
                used_words
            };

            let over_chars = params.chars.map(|b| next_chars > b).unwrap_or(false);
            let over_words = params.words.map(|b| next_words > b).unwrap_or(false);
            if over_chars || over_words {
                break;
            }

            used_chars = next_chars;
            used_words = next_words;
            last_good_byte = offset + segment.len();
        }

        // If no progress at all, take the next whole segment to avoid
        // an infinite loop (can happen if the first segment's char
        // count already exceeds the budget).
        if last_good_byte == 0 {
            if let Some((_, segment)) = slice.split_word_bound_indices().next() {
                last_good_byte = segment.len();
            } else {
                break;
            }
        }

        let window = slice[..last_good_byte].to_string();
        if !window.is_empty() {
            out.push(window);
        }

        // Advance cursor by the full window length, then step back by
        // `overlap` chars (to a word boundary) for the next window.
        // Guarantee forward progress so we can't loop forever.
        let next_cursor = cursor + last_good_byte;
        let candidate = advance_with_overlap(&rendered, next_cursor, params.overlap);
        cursor = if candidate <= cursor {
            next_cursor
        } else {
            candidate
        };
    }

    out
}

/// Step the cursor back by roughly `overlap` characters, then land on
/// the next word boundary to keep slices aligned. Returns the new
/// cursor position (byte offset).
fn advance_with_overlap(rendered: &str, end_byte: usize, overlap: usize) -> usize {
    if overlap == 0 {
        return end_byte;
    }
    let prefix = &rendered[..end_byte];
    let prefix_chars = prefix.chars().count();
    if overlap >= prefix_chars {
        return 0;
    }
    let target_char_index = prefix_chars - overlap;

    // Find the word boundary at or before target_char_index.
    let mut char_idx: usize = 0;
    let mut last_boundary: usize = 0;
    for (offset, segment) in prefix.split_word_bound_indices() {
        if char_idx >= target_char_index {
            return offset;
        }
        char_idx += segment.chars().count();
        last_boundary = offset + segment.len();
    }
    last_boundary
}

fn build_result(ruby: &Ruby, windows: &[String], with_counts: bool) -> Result<RArray, Error> {
    let arr = ruby.ary_new_capa(windows.len());
    for (i, content) in windows.iter().enumerate() {
        let hash = ruby.hash_new();
        hash.aset(ruby.to_symbol("index"), i)?;
        hash.aset(ruby.to_symbol("content"), content.as_str())?;
        if with_counts {
            hash.aset(
                ruby.to_symbol("character_count"),
                content.trim().chars().count(),
            )?;
            hash.aset(
                ruby.to_symbol("word_count"),
                content.unicode_words().count(),
            )?;
        }
        arr.push(hash)?;
    }
    Ok(arr)
}
