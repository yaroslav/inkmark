//! Serialize pulldown-cmark events to plain text.
//!
//! Designed for embedding models, token counting, and any pipeline
//! where Markdown syntax is noise. Runs after the normal filter
//! pipeline (emoji replacement, autolink, host/scheme allowlists), so
//! the caller already sees resolved emoji, unwrapped disallowed links,
//! and so on.
//!
//! Core idea: **buffer stack**. Most writes go to the top-of-stack
//! buffer. Contexts that need post-processing (blockquote line
//! prefixing, link `text (url)` formatting, image alt capture,
//! footnote body capture) open a fresh buffer at the Start event and
//! pop + format at End. Nested contexts fall out for free because the
//! stack naturally tracks nesting depth.

use pulldown_cmark::{Event, Tag, TagEnd};

/// Write plain-text output into `buf` from a pulldown-cmark event stream.
pub fn write_plain_text<'a, I: IntoIterator<Item = Event<'a>>>(events: I, buf: &mut String) {
    let mut w = Writer::new();
    for event in events {
        w.handle(event);
    }
    let out = w.finalize();
    buf.push_str(&out);
}

struct Writer {
    /// Stack of write targets. Always non-empty; top is the current
    /// target. `open()` pushes, `close()` pops.
    buffers: Vec<String>,
    list_stack: Vec<ListCtx>,
    link_dest: String,
    image_dest: String,
    footnote_label: String,
    /// Accumulated definitions, emitted at `finalize` in document order.
    footnote_bodies: Vec<(String, String)>,
    /// Current row's cells, tab-joined at TableRow/TableHead End.
    current_row: Vec<String>,
}

struct ListCtx {
    ordered: bool,
    counter: u64,
    indent: usize,
}

impl Writer {
    fn new() -> Self {
        Self {
            buffers: vec![String::new()],
            list_stack: Vec::new(),
            link_dest: String::new(),
            image_dest: String::new(),
            footnote_label: String::new(),
            footnote_bodies: Vec::new(),
            current_row: Vec::new(),
        }
    }

    fn write(&mut self, s: &str) {
        self.buffers
            .last_mut()
            .expect("buffer stack is never empty")
            .push_str(s);
    }

    fn open(&mut self) {
        self.buffers.push(String::new());
    }

    fn close(&mut self) -> String {
        self.buffers.pop().expect("close() without matching open()")
    }

    /// Ensure the current buffer ends with exactly one blank line
    /// (i.e. `"\n\n"`), except when the buffer is empty (no leading
    /// newlines at document or subtree start).
    fn ensure_blank_line(&mut self) {
        let buf = self.buffers.last().expect("buffer stack is never empty");
        if buf.is_empty() || buf.ends_with("\n\n") {
            return;
        }
        if buf.ends_with('\n') {
            self.write("\n");
        } else {
            self.write("\n\n");
        }
    }

    /// Ensure the current buffer ends with `\n`. Used for transitions
    /// that should just break the current line without introducing
    /// paragraph-style separation (e.g. a nested list inside a list
    /// item: `- outer\n  - inner`, not a blank line between them).
    fn ensure_newline(&mut self) {
        let buf = self.buffers.last().expect("buffer stack is never empty");
        if buf.is_empty() || buf.ends_with('\n') {
            return;
        }
        self.write("\n");
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(end) => self.end(end),
            Event::Text(t) | Event::Code(t) => self.write(&t),
            Event::SoftBreak => self.write(" "),
            Event::HardBreak => self.write("\n"),
            Event::Rule => {
                self.ensure_blank_line();
                self.write("---\n\n");
            }
            // Raw HTML reaches us only when raw_html: true (the
            // suppress_raw_html filter rewrites it to Event::Text
            // otherwise). Emit it verbatim to mirror the to_html /
            // to_markdown contract.
            Event::Html(h) | Event::InlineHtml(h) => self.write(&h),
            Event::FootnoteReference(label) => {
                self.write("[");
                self.write(&label);
                self.write("]");
            }
            // Task-list markers are dropped; the item bullet remains.
            Event::TaskListMarker(_) => {}
            Event::InlineMath(t) | Event::DisplayMath(t) => self.write(&t),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { .. } => self.ensure_blank_line(),
            Tag::BlockQuote(_) => {
                self.ensure_blank_line();
                self.open();
            }
            Tag::CodeBlock(_) => {
                self.ensure_blank_line();
                self.open();
            }
            Tag::List(first) => {
                // Nested lists separate with a single newline (appear
                // as the next line of their parent item); top-level
                // lists get paragraph-style blank-line separation.
                if self.list_stack.is_empty() {
                    self.ensure_blank_line();
                } else {
                    self.ensure_newline();
                }
                let indent = self.list_stack.len() * 2;
                self.list_stack.push(ListCtx {
                    ordered: first.is_some(),
                    counter: first.unwrap_or(1),
                    indent,
                });
            }
            Tag::Item => {
                let ctx = self.list_stack.last_mut().expect("item outside list");
                let indent = " ".repeat(ctx.indent);
                let bullet = if ctx.ordered {
                    let n = ctx.counter;
                    ctx.counter += 1;
                    format!("{}. ", n)
                } else {
                    "- ".to_string()
                };
                self.write(&indent);
                self.write(&bullet);
            }
            Tag::Table(_) => self.ensure_blank_line(),
            Tag::TableHead | Tag::TableRow => {}
            Tag::TableCell => self.open(),
            Tag::Link { dest_url, .. } => {
                self.link_dest = dest_url.to_string();
                self.open();
            }
            Tag::Image { dest_url, .. } => {
                self.image_dest = dest_url.to_string();
                self.open();
            }
            Tag::Emphasis | Tag::Strong | Tag::Strikethrough => {}
            Tag::FootnoteDefinition(label) => {
                self.footnote_label = label.to_string();
                self.open();
            }
            // YAML metadata: buffer + discard on End so the raw
            // frontmatter never reaches plain-text output (the Ruby
            // side consumes it separately via `frontmatter`).
            Tag::MetadataBlock(_) => self.open(),
            // Pass-through structural tags—inner content writes to
            // the current buffer unchanged.
            Tag::HtmlBlock
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Subscript
            | Tag::Superscript => {}
        }
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.write("\n\n"),
            TagEnd::BlockQuote(_) => {
                let inner = self.close();
                let prefixed = prefix_lines(inner.trim_end_matches('\n'), "> ");
                self.write(&prefixed);
                self.write("\n\n");
            }
            TagEnd::CodeBlock => {
                let inner = self.close();
                self.write(&inner);
                self.ensure_blank_line();
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
                // Only paragraph-separate after top-level lists; inside
                // a parent item we're about to hit End(Item), which
                // writes its own `\n`.
                if self.list_stack.is_empty() {
                    self.ensure_blank_line();
                } else {
                    self.ensure_newline();
                }
            }
            TagEnd::Item => self.write("\n"),
            TagEnd::Table => self.write("\n"),
            TagEnd::TableHead => {
                let row = std::mem::take(&mut self.current_row).join("\t");
                self.write(&row);
                // Blank line between header and body for readability.
                self.write("\n\n");
            }
            TagEnd::TableRow => {
                let row = std::mem::take(&mut self.current_row).join("\t");
                self.write(&row);
                self.write("\n");
            }
            TagEnd::TableCell => {
                let cell = self.close();
                self.current_row.push(cell);
            }
            TagEnd::Link => {
                let text = self.close();
                // Collapse when link text equals its URL (autolinks
                // like `<https://x>` or linkify-produced links).
                if text == self.link_dest {
                    self.write(&text);
                } else {
                    self.write(&text);
                    self.write(" (");
                    let dest = std::mem::take(&mut self.link_dest);
                    self.write(&dest);
                    self.write(")");
                }
            }
            TagEnd::Image => {
                let alt = self.close();
                self.write(&alt);
                self.write(" (");
                let dest = std::mem::take(&mut self.image_dest);
                self.write(&dest);
                self.write(")");
            }
            TagEnd::FootnoteDefinition => {
                let body = self.close();
                let label = std::mem::take(&mut self.footnote_label);
                self.footnote_bodies.push((label, body.trim().to_string()));
            }
            TagEnd::MetadataBlock(_) => {
                let _ = self.close();
            }
            _ => {}
        }
    }

    fn finalize(mut self) -> String {
        if !self.footnote_bodies.is_empty() {
            self.ensure_blank_line();
            let defs = std::mem::take(&mut self.footnote_bodies);
            for (i, (label, body)) in defs.iter().enumerate() {
                if i > 0 {
                    self.write("\n");
                }
                self.write("[");
                self.write(label);
                self.write("]: ");
                self.write(body);
            }
            self.write("\n");
        }
        let mut out = self.buffers.pop().expect("buffer stack is never empty");
        // Trim trailing blank lines down to one final newline.
        while out.ends_with("\n\n") {
            out.pop();
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// Prefix every line of `s` with `prefix`. Empty lines receive the
/// prefix with its trailing whitespace stripped—so a `"> "` prefix
/// on a blank line produces `>`, matching email quoting convention.
fn prefix_lines(s: &str, prefix: &str) -> String {
    let trimmed_prefix = prefix.trim_end();
    let mut out = String::with_capacity(s.len() + prefix.len() * 4);
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.is_empty() {
            out.push_str(trimmed_prefix);
        } else {
            out.push_str(prefix);
            out.push_str(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::{Options, Parser};

    fn plain(md: &str) -> String {
        let mut buf = String::new();
        let opts = Options::ENABLE_GFM
            | Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES;
        write_plain_text(Parser::new_ext(md, opts), &mut buf);
        buf
    }

    #[test]
    fn paragraph_strips_emphasis() {
        assert_eq!(
            plain("**bold** and *italic* and ~~strike~~"),
            "bold and italic and strike\n"
        );
    }

    #[test]
    fn link_expands_to_text_with_url() {
        assert_eq!(
            plain("[example](https://example.net)"),
            "example (https://example.net)\n"
        );
    }

    #[test]
    fn autolink_collapses_text_equals_url() {
        assert_eq!(plain("<https://example.net>"), "https://example.net\n");
    }

    #[test]
    fn image_emits_alt_and_src() {
        assert_eq!(plain("![cat](cat.png)"), "cat (cat.png)\n");
    }

    #[test]
    fn heading_is_plain_text_with_blank_line() {
        assert_eq!(plain("# Title\n\nBody"), "Title\n\nBody\n");
    }

    #[test]
    fn blockquote_prefixes_lines() {
        let out = plain("> hello\n> world");
        assert_eq!(out, "> hello world\n");
    }

    #[test]
    fn nested_blockquote_double_prefix() {
        let out = plain("> > nested");
        assert_eq!(out, "> > nested\n");
    }

    #[test]
    fn blockquote_with_blank_line_uses_bare_marker() {
        let out = plain("> first\n>\n> second");
        assert_eq!(out, "> first\n>\n> second\n");
    }

    #[test]
    fn unordered_list_dash_bullet() {
        assert_eq!(plain("- a\n- b"), "- a\n- b\n");
    }

    #[test]
    fn ordered_list_numbers() {
        assert_eq!(plain("1. first\n2. second"), "1. first\n2. second\n");
    }

    #[test]
    fn nested_list_indented_two_spaces() {
        let out = plain("- outer\n  - inner");
        assert_eq!(out, "- outer\n  - inner\n");
    }

    #[test]
    fn tasklist_drops_checkbox() {
        assert_eq!(plain("- [x] done\n- [ ] todo"), "- done\n- todo\n");
    }

    #[test]
    fn table_header_blank_line_then_body() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
        let out = plain(md);
        assert_eq!(out, "a\tb\n\n1\t2\n3\t4\n");
    }

    #[test]
    fn code_block_preserved_verbatim() {
        let out = plain("```ruby\nputs \"hi\"\n```");
        assert_eq!(out, "puts \"hi\"\n");
    }

    #[test]
    fn horizontal_rule_emits_dashes() {
        assert_eq!(plain("before\n\n---\n\nafter"), "before\n\n---\n\nafter\n");
    }

    #[test]
    fn footnote_reference_and_definition() {
        let md = "See[^x].\n\n[^x]: body text";
        let out = plain(md);
        assert_eq!(out, "See[x].\n\n[x]: body text\n");
    }

    #[test]
    fn inline_code_strips_backticks() {
        assert_eq!(plain("use `puts` please"), "use puts please\n");
    }

    #[test]
    fn hard_break_is_newline() {
        assert_eq!(plain("line1  \nline2"), "line1\nline2\n");
    }
}
