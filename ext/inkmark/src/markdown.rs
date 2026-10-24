//! Markdown serialization through `pulldown-cmark-to-cmark`.
//!
//! Every Markdown-emitting API (`to_markdown`, `chunks_by_heading`,
//! `chunks_by_size`, `truncate_markdown`) funnels through this module so
//! the serializer always sees the parser options that produced the
//! events: pulldown-cmark-to-cmark uses them to pick escapes that reparse
//! to the same document.
//!
//! Serializer failures come back as `Err` instead of panicking (a panic
//! reaches Ruby as `fatal`, which cannot be rescued). Native entry points
//! turn them into `Inkmark::Error` via [`to_ruby_error`].
//!
//! Two entry points: [`write`] streams parser output straight through
//! (the unfiltered fast path); [`render`] takes filtered events and falls
//! back to [`tidy_inline_edges`] when the serializer rejects them.

use std::collections::VecDeque;

use magnus::{prelude::*, Error, ExceptionClass, RClass, Ruby};
use pulldown_cmark::{CowStr, Event, Options, Tag, TagEnd};

use crate::options::Flags;

pub use pulldown_cmark_to_cmark::Error as SerializeError;

/// Options to serialize a filtered document with: the parser options it
/// was parsed with, plus what the filters' output needs. The `heading_ids`
/// filter gives headings an `id`, which Markdown can only express as a
/// `{ #id }` heading attribute, so heading attributes are switched on for
/// the serializer (matching 0.2's output) instead of it failing on
/// headings it cannot otherwise preserve.
pub fn serializer_options(cm_opts: Options, flags: &Flags) -> Options {
    if flags.heading_ids {
        cm_opts | Options::ENABLE_HEADING_ATTRIBUTES
    } else {
        cm_opts
    }
}

/// Serialize unfiltered parser output into `buf`. `md_opts` must be the
/// options the events were parsed with.
pub fn write<'a, I>(events: I, buf: &mut String, md_opts: Options) -> Result<(), SerializeError>
where
    I: Iterator<Item = Event<'a>>,
{
    pulldown_cmark_to_cmark::cmark(InertHtmlBlocks::new(events), buf, md_opts)?;
    Ok(())
}

/// Serialize filtered events into a new String. `md_opts` comes from
/// [`serializer_options`].
///
/// Filters can leave shapes the serializer cannot write (see
/// [`tidy_inline_edges`]). Parser output never has them, so events are
/// reshaped only after a rejection, then serialized once more: output that
/// serializes as-is is never touched.
pub fn render(events: &[Event<'_>], md_opts: Options) -> Result<String, SerializeError> {
    let mut buf = String::new();
    if write(events.iter().cloned(), &mut buf, md_opts).is_ok() {
        return Ok(buf);
    }
    let mut events = events.to_vec();
    tidy_inline_edges(&mut events);
    let mut buf = String::new();
    write(events.into_iter(), &mut buf, md_opts)?;
    Ok(buf)
}

/// Blank line that rejoins top-level blocks serialized one at a time.
/// Without it, adjacent paragraphs would merge on reparse.
pub const BLOCK_SEPARATOR: &str = "\n\n";

/// Serialize one top-level block for joining with [`BLOCK_SEPARATOR`]:
/// rendered alone, minus the newlines it starts or ends with.
pub fn render_block(events: &[Event<'_>], md_opts: Options) -> Result<String, SerializeError> {
    Ok(render(events, md_opts)?.trim_matches('\n').to_owned())
}

/// Convert a serializer failure into an `Inkmark::Error` exception.
///
/// The class is looked up on each call rather than cached: the extension
/// keeps no Ruby `VALUE`s in process-global state (see `rb_ext_ractor_safe`
/// in `lib.rs`), and this path only runs on failure.
pub fn to_ruby_error(ruby: &Ruby, err: SerializeError) -> Error {
    let class = ruby
        .class_object()
        .const_get::<_, RClass>("Inkmark")
        .and_then(|inkmark| inkmark.const_get::<_, ExceptionClass>("Error"));
    match class {
        Ok(class) => Error::new(class, format!("markdown serialization failed: {err}")),
        Err(lookup_failed) => lookup_failed,
    }
}

/// Inline containers written as delimiter runs (`*`, `**`, `~~`, `^`, `~`).
/// CommonMark only recognizes a run that hugs non-whitespace, so these
/// cannot be empty or start or end with whitespace or a line break.
fn is_delimited_start(event: Option<&Event<'_>>) -> bool {
    matches!(
        event,
        Some(Event::Start(
            Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Superscript | Tag::Subscript
        ))
    )
}

fn is_delimited_end(end: &TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
    )
}

/// Block containers: a line break cannot open or close one.
fn is_block_start(event: Option<&Event<'_>>) -> bool {
    matches!(event, Some(Event::Start(tag)) if !matches!(tag,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Superscript
            | Tag::Subscript | Tag::Link { .. } | Tag::Image { .. }))
}

fn is_block_end(end: &TagEnd) -> bool {
    !is_delimited_end(end) && !matches!(end, TagEnd::Link | TagEnd::Image)
}

fn is_break(event: Option<&Event<'_>>) -> bool {
    matches!(event, Some(Event::SoftBreak | Event::HardBreak))
}

/// What the parser strips at both ends of every line of inline content.
fn is_line_space(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// A line of inline content starts after this event.
fn opens_line(event: Option<&Event<'_>>) -> bool {
    is_break(event)
        || matches!(
            event,
            Some(Event::Start(
                Tag::Paragraph | Tag::Heading { .. } | Tag::TableCell | Tag::Item
            ))
        )
}

/// A line of inline content ends at this tag.
fn closes_line(end: &TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell | TagEnd::Item
    )
}

fn last_text_ends_with_line_space(out: &[Event<'_>]) -> bool {
    matches!(out.last(), Some(Event::Text(t)) if t.ends_with(is_line_space))
}

fn trim_last_text_end(out: &mut Vec<Event<'_>>) {
    if let Some(Event::Text(text)) = out.pop() {
        let body = text.trim_end_matches(is_line_space);
        if !body.is_empty() {
            out.push(Event::Text(CowStr::from(body.to_owned())));
        }
    }
}

/// Reshape filtered events the serializer could not otherwise write.
///
/// Filters can leave a delimited container with whitespace at an edge or
/// with nothing inside, typically by unwrapping a disallowed link or image
/// so its text lands directly inside `**...**`. Line breaks and whitespace
/// at an edge move outside the container, emptied containers are dropped,
/// a line break left opening or closing a block is dropped, and spaces or
/// tabs left at the start or end of a line are trimmed. Rendering is
/// unchanged (`<strong>a </strong>` displays like `<strong>a</strong> `).
fn tidy_inline_edges(events: &mut Vec<Event<'_>>) {
    // Each pass fixes one nesting level per edge; repeat until stable.
    // Every change shortens the list or moves an event strictly outward,
    // so this terminates.
    while tidy_pass(events) {}
}

fn tidy_pass(events: &mut Vec<Event<'_>>) -> bool {
    let mut changed = false;
    let mut out: Vec<Event<'_>> = Vec::with_capacity(events.len());
    for event in events.drain(..) {
        match event {
            Event::End(end) if is_delimited_end(&end) && is_delimited_start(out.last()) => {
                out.pop();
                changed = true;
            }
            Event::End(end) if is_delimited_end(&end) && is_break(out.last()) => {
                let brk = out.pop().expect("checked non-empty");
                out.push(Event::End(end));
                out.push(brk);
                changed = true;
            }
            Event::End(end)
                if is_delimited_end(&end)
                    && matches!(out.last(), Some(Event::Text(t)) if t.ends_with(char::is_whitespace)) =>
            {
                let Some(Event::Text(text)) = out.pop() else {
                    unreachable!("checked above")
                };
                let body = text.trim_end();
                let space = text[body.len()..].to_owned();
                if !body.is_empty() {
                    out.push(Event::Text(CowStr::from(body.to_owned())));
                }
                out.push(Event::End(end));
                out.push(Event::Text(CowStr::from(space)));
                changed = true;
            }
            Event::End(end) if is_block_end(&end) && is_break(out.last()) => {
                out.pop();
                out.push(Event::End(end));
                changed = true;
            }
            Event::SoftBreak | Event::HardBreak if is_delimited_start(out.last()) => {
                let start = out.pop().expect("checked non-empty");
                out.push(event);
                out.push(start);
                changed = true;
            }
            Event::SoftBreak | Event::HardBreak if is_block_start(out.last()) => {
                changed = true;
            }
            Event::Text(text)
                if text.starts_with(char::is_whitespace) && is_delimited_start(out.last()) =>
            {
                let start = out.pop().expect("checked non-empty");
                let body = text.trim_start();
                let space = text[..text.len() - body.len()].to_owned();
                out.push(Event::Text(CowStr::from(space)));
                out.push(start);
                if !body.is_empty() {
                    out.push(Event::Text(CowStr::from(body.to_owned())));
                }
                changed = true;
            }
            Event::End(end) if closes_line(&end) && last_text_ends_with_line_space(&out) => {
                trim_last_text_end(&mut out);
                out.push(Event::End(end));
                changed = true;
            }
            Event::SoftBreak | Event::HardBreak if last_text_ends_with_line_space(&out) => {
                trim_last_text_end(&mut out);
                out.push(event);
                changed = true;
            }
            Event::Text(text) if text.starts_with(is_line_space) && opens_line(out.last()) => {
                let body = text.trim_start_matches(is_line_space);
                if !body.is_empty() {
                    out.push(Event::Text(CowStr::from(body.to_owned())));
                }
                changed = true;
            }
            other => out.push(other),
        }
    }
    *events = out;
    changed
}

/// Keeps suppressed raw HTML inert in Markdown output.
///
/// `suppress_raw_html` turns `Html` events into `Text` but leaves the
/// `HtmlBlock` container around them, which is what the HTML renderer
/// wants (escaped text, no `<p>`). The Markdown serializer, though, writes
/// an `HtmlBlock`'s contents verbatim, so that text would reparse as live
/// HTML. Blocks whose contents start with `Text` are rewritten as a
/// paragraph, one trimmed line per `SoftBreak`, so the serializer escapes
/// them. Blocks of real `Html` events (`raw_html: true`) pass through.
struct InertHtmlBlocks<'a, I: Iterator<Item = Event<'a>>> {
    inner: std::iter::Peekable<I>,
    pending: VecDeque<Event<'a>>,
}

impl<'a, I: Iterator<Item = Event<'a>>> InertHtmlBlocks<'a, I> {
    fn new(inner: I) -> Self {
        Self {
            inner: inner.peekable(),
            pending: VecDeque::new(),
        }
    }

    /// Consume the rest of a suppressed HTML block (through its `End`) and
    /// queue it as a paragraph.
    fn queue_as_paragraph(&mut self) {
        let mut lines: Vec<String> = Vec::new();
        loop {
            match self.inner.next() {
                // An `Html` event in a block that was otherwise suppressed
                // is escaped too: staying inert is the safe default.
                Some(Event::Text(t)) | Some(Event::Html(t)) => lines.extend(
                    t.split('\n')
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_owned),
                ),
                Some(Event::End(TagEnd::HtmlBlock)) | None => break,
                Some(_) => {}
            }
        }
        if lines.is_empty() {
            return;
        }
        self.pending.push_back(Event::Start(Tag::Paragraph));
        for (i, line) in lines.into_iter().enumerate() {
            if i > 0 {
                self.pending.push_back(Event::SoftBreak);
            }
            self.pending.push_back(Event::Text(CowStr::from(line)));
        }
        self.pending.push_back(Event::End(TagEnd::Paragraph));
    }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for InertHtmlBlocks<'a, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Event<'a>> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(event);
            }
            match self.inner.next()? {
                Event::Start(Tag::HtmlBlock)
                    if matches!(self.inner.peek(), Some(Event::Text(_))) =>
                {
                    // May queue nothing (all-blank block); loop either way.
                    self.queue_as_paragraph();
                }
                event => return Some(event),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    fn suppressed(source: &str) -> Vec<Event<'_>> {
        Parser::new_ext(source, Options::empty())
            .map(|e| match e {
                Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
                other => other,
            })
            .collect()
    }

    fn reparse_has_html(markdown: &str) -> bool {
        Parser::new_ext(markdown, Options::empty())
            .any(|e| matches!(e, Event::Html(_) | Event::InlineHtml(_)))
    }

    #[test]
    fn suppressed_html_block_stays_inert() {
        let md = render(
            &suppressed("<div onclick=\"x()\">\n*inside*\n</div>\n"),
            Options::empty(),
        )
        .unwrap();
        assert!(!reparse_has_html(&md), "reparsed as live HTML: {md:?}");
        assert!(md.contains("div"), "lost the escaped text: {md:?}");
    }

    #[test]
    fn suppressed_html_block_drops_blank_lines() {
        // HTML block types 1-5 (here <script>) may contain blank lines,
        // which would end a paragraph on reparse.
        let md = render(
            &suppressed("<script>\n\nalert(1)\n</script>\n"),
            Options::empty(),
        )
        .unwrap();
        assert!(!reparse_has_html(&md), "reparsed as live HTML: {md:?}");
        assert!(md.contains("alert(1)"), "lost the escaped text: {md:?}");
    }

    #[test]
    fn live_html_block_passes_through() {
        let events: Vec<Event> =
            Parser::new_ext("<div>\nraw\n</div>\n", Options::empty()).collect();
        let md = render(&events, Options::empty()).unwrap();
        assert!(md.contains("<div>"), "raw HTML was rewritten: {md:?}");
    }

    /// Parse `source` and drop every link's Start/End, as the allowlist
    /// filters do when they unwrap a disallowed link to its text.
    fn unwrapped_links(source: &str) -> Vec<Event<'_>> {
        Parser::new_ext(source, Options::empty())
            .filter(|e| !matches!(e, Event::Start(Tag::Link { .. }) | Event::End(TagEnd::Link)))
            .collect()
    }

    #[test]
    fn unwrapped_link_edges_inside_emphasis_serialize() {
        for source in [
            "**[text\n](u)**",
            "**[\ntext](u)**",
            "**[text ](u)**",
            "**[ text](u)**",
            "**_[text\n](u)_**",
            "a **[text ](u)** b",
        ] {
            let md = render(&unwrapped_links(source), Options::empty())
                .unwrap_or_else(|e| panic!("{source:?} failed: {e}"));
            let strong = Parser::new_ext(&md, Options::empty())
                .any(|e| matches!(e, Event::Start(Tag::Strong)));
            assert!(strong && md.contains("text"), "{source:?} => {md:?}");
        }
    }

    #[test]
    fn emphasis_emptied_by_unwrapping_is_dropped() {
        let md = render(&unwrapped_links("a *[](u)* b"), Options::empty()).unwrap();
        assert!(!md.contains('*'), "{md:?}");
    }

    #[test]
    fn unbalanced_events_return_an_error_instead_of_panicking() {
        let events = vec![
            Event::Start(Tag::Paragraph),
            Event::Text(CowStr::Borrowed("x")),
            Event::End(TagEnd::Paragraph),
            Event::End(TagEnd::BlockQuote(None)),
        ];
        assert!(render(&events, Options::empty()).is_err());
    }
}
