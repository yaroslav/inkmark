//! Markdown truncation for LLM / RAG pipelines.
//!
//! Cuts a filter-applied event stream at either a block boundary
//! (`TruncateAt::Block`) or a Unicode word boundary
//! (`TruncateAt::Word`), respecting optional character and word
//! budgets. Designed as a first-stage preprocessor for embedding
//! input, context-window budgeting, and chunk normalization.

use magnus::{Error, RHash, Ruby};
use pulldown_cmark::{Event, Parser};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::apply_filters;
use crate::options::{build_options, Flags};

/// What kind of boundary to cut at.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TruncateAt {
    /// Last top-level Markdown block that fits the budget. Output is
    /// always valid Markdown.
    Block,
    /// Last Unicode word boundary that fits the budget. Output is a
    /// Markdown string but may split an open construct (code fence,
    /// link, emphasis).
    Word,
}

/// Parameters for a truncation pass. At least one of `chars` / `words`
/// must be `Some`; if both are set, cut when the first of the two
/// budgets is exhausted. `marker` ("...") counts toward the budget:
/// if supplied, the effective content budget is reduced so that final
/// output length stays at or under the user-given limit.
pub struct TruncateParams {
    pub chars: Option<usize>,
    pub words: Option<usize>,
    pub at: TruncateAt,
    pub marker: Option<String>,
}

/// Full-document entry point: parse + filter + truncate.
pub fn truncate_source(
    source: &str,
    cm_opts: pulldown_cmark::Options,
    flags: &Flags,
    params: &TruncateParams,
) -> String {
    let events: Vec<Event> = Parser::new_ext(source, cm_opts).collect();
    let events = apply_filters(events, flags);
    truncate_events(&events, params)
}

/// Ruby-facing entry point. Expects `params_hash` to contain
/// `:chars`, `:words`, `:at` (`:block` | `:word`), and `:marker`
/// (a String or nil). Argument validation lives on the Ruby side;
/// we just read the values defensively here.
pub fn native_truncate_markdown(
    ruby: &Ruby,
    source: String,
    params_hash: RHash,
    opts_hash: RHash,
) -> Result<String, Error> {
    let (cm_opts, flags) = build_options(ruby, opts_hash)?;
    let params = parse_params(ruby, params_hash)?;
    Ok(truncate_source(&source, cm_opts, &flags, &params))
}

pub fn parse_params(ruby: &Ruby, hash: RHash) -> Result<TruncateParams, Error> {
    let chars: Option<usize> = hash.lookup(ruby.to_symbol("chars"))?;
    let words: Option<usize> = hash.lookup(ruby.to_symbol("words"))?;
    let at_sym: Option<String> = hash.lookup(ruby.to_symbol("at"))?;
    let marker: Option<String> = hash.lookup(ruby.to_symbol("marker"))?;

    let at = match at_sym.as_deref() {
        Some("block") => TruncateAt::Block,
        Some("word") => TruncateAt::Word,
        _ => TruncateAt::Block,
    };

    Ok(TruncateParams {
        chars,
        words,
        at,
        marker,
    })
}

/// Core truncation over a filter-applied event slice.
pub fn truncate_events(events: &[Event<'_>], params: &TruncateParams) -> String {
    match params.at {
        TruncateAt::Block => truncate_at_block(events, params),
        TruncateAt::Word => truncate_at_word(events, params),
    }
}

fn truncate_at_block(events: &[Event<'_>], params: &TruncateParams) -> String {
    let marker_chars = marker_chars(&params.marker);
    let marker_words = marker_words(&params.marker);
    let char_budget = params.chars.map(|n| n.saturating_sub(marker_chars));
    let word_budget = params.words.map(|n| n.saturating_sub(marker_words));

    let mut kept = String::new();
    let mut used_chars: usize = 0;
    let mut used_words: usize = 0;
    let mut any_dropped = false;

    for (start, end) in top_level_blocks(events) {
        let block = render_markdown(&events[start..=end]);
        let block_chars = block.trim_end().chars().count();
        let block_words = block.unicode_words().count();

        let would_exceed_chars = char_budget
            .map(|b| used_chars + block_chars > b)
            .unwrap_or(false);
        let would_exceed_words = word_budget
            .map(|b| used_words + block_words > b)
            .unwrap_or(false);

        if would_exceed_chars || would_exceed_words {
            any_dropped = true;
            break;
        }

        kept.push_str(&block);
        used_chars += block_chars;
        used_words += block_words;
    }

    // If nothing was dropped and source fit, return unchanged (no marker).
    if !any_dropped {
        return kept;
    }

    // First block alone exceeded the budget; honest empty return
    if kept.is_empty() {
        return String::new();
    }

    // Trim trailing whitespace before appending the marker so the
    // ellipsis attaches cleanly to the last block.
    while kept.ends_with(|c: char| c.is_whitespace()) {
        kept.pop();
    }
    if let Some(marker) = &params.marker {
        kept.push_str("\n\n");
        kept.push_str(marker);
    }
    kept.push('\n');
    kept
}

/// Return (start, end) event-index pairs for top-level blocks.
/// A block starts at an `Event::Start` with depth 0 and ends at the
/// matching `Event::End`. Leaf events at depth 0 (Rule, HardBreak if
/// it ever happens, raw Html block) count as single-event blocks.
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
                    // Standalone top-level event (Event::Rule, etc.).
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

fn truncate_at_word(events: &[Event<'_>], params: &TruncateParams) -> String {
    let rendered = render_markdown(events);
    let marker_chars = marker_chars(&params.marker);
    let marker_words = marker_words(&params.marker);

    let total_chars = rendered.chars().count();
    let total_words = rendered.unicode_words().count();

    let chars_ok = params
        .chars
        .map(|limit| total_chars + marker_chars <= limit)
        .unwrap_or(true);
    let words_ok = params
        .words
        .map(|limit| total_words + marker_words <= limit)
        .unwrap_or(true);

    // Fits under both budgets: return unchanged, no marker.
    if chars_ok && words_ok {
        return rendered;
    }

    let char_budget = params.chars.map(|n| n.saturating_sub(marker_chars));
    let word_budget = params.words.map(|n| n.saturating_sub(marker_words));

    // Walk word boundaries, tracking cumulative char and word counts.
    // `last_good_end` is the byte offset of the end of the last word
    // segment that stays within both budgets.
    let mut last_good_end: usize = 0;
    let mut used_chars: usize = 0;
    let mut used_words: usize = 0;

    for (offset, segment) in rendered.split_word_bound_indices() {
        let seg_chars = segment.chars().count();
        let seg_is_word = segment.unicode_words().next().is_some();
        let next_words = if seg_is_word {
            used_words + 1
        } else {
            used_words
        };
        let next_chars = used_chars + seg_chars;

        let over_chars = char_budget.map(|b| next_chars > b).unwrap_or(false);
        let over_words = word_budget.map(|b| next_words > b).unwrap_or(false);
        if over_chars || over_words {
            break;
        }

        used_chars = next_chars;
        used_words = next_words;
        last_good_end = offset + segment.len();
    }

    let mut out = rendered[..last_good_end].to_string();
    while out.ends_with(|c: char| c.is_whitespace()) {
        out.pop();
    }
    if let Some(marker) = &params.marker {
        out.push_str(marker);
    }
    out
}

fn marker_chars(marker: &Option<String>) -> usize {
    marker.as_ref().map(|m| m.chars().count()).unwrap_or(0)
}

fn marker_words(marker: &Option<String>) -> usize {
    marker
        .as_ref()
        .map(|m| m.unicode_words().count())
        .unwrap_or(0)
}
