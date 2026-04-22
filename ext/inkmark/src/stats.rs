//! Document statistics and table-of-contents collector.
//!
//! Walks a slice of `(Event, byte_range)` tuples once (before filters) and collects:
//! - Text buffer → character count, word count, language detection
//! - Heading entries → heading count, TOC (markdown + HTML), heading extract
//! - Code block count + raw source extract
//! - Image and link extract metadata
//! - Footnote definition count + body extract
//!
//! Byte ranges come from pulldown-cmark's `OffsetIter`: the Start tag's
//! range spans the whole source element (e.g. the entire
//! `` ```ruby\n...\n``` ``
//! for a fenced code block). The caller (`document.rs`) parses with
//! `Parser::new_ext(...).into_offset_iter()` and hands us the result.
//!
//! The collector is the single source of truth for the full-render path.
//! Two independent Ruby-side knobs consume its output:
//! - `statistics: true` => scalar counts and language
//! detection (`to_statistics_hash`)
//! - `extract: {...}` => filtered arrays of structured
//! records (`to_extracts_hash`)

use std::ops::Range;

use magnus::{Error, RArray, RHash, Ruby};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use unicode_segmentation::UnicodeSegmentation;

use crate::heading::{self, SlugDeduplicator};
use crate::toc::{self, TocEntry};

pub struct ImageInfo {
    pub src: String,
    pub alt: String,
    pub title: String,
    pub byte_range: Range<usize>,
}

pub struct LinkInfo {
    pub href: String,
    pub text: String,
    pub title: String,
    pub byte_range: Range<usize>,
}

/// A single fenced or indented code block captured before any filter
/// runs: the `source` is pulldown-cmark's unmodified content, suitable
/// for passing to an external highlighter.
/// `lang` is the info string on a fence (e.g. `"ruby"`); indented code
/// blocks carry the empty string, matching the handler API.
pub struct CodeBlockInfo {
    pub lang: String,
    pub source: String,
    pub byte_range: Range<usize>,
}

/// A footnote definition `[^label]: body`. `text` is the plain-text body:
/// emphasis, links, and inline formatting are flattened to their text
/// content, matching how `ImageInfo.alt` and `LinkInfo.text` are captured.
pub struct FootnoteDefInfo {
    pub label: String,
    pub text: String,
    pub byte_range: Range<usize>,
}

/// Heading record for the `extract[:headings]` projection. Parallel to
/// `toc::TocEntry` but adds a byte range and uses the `id`/extract
/// vocabulary. We push to both during the walk: it's one allocation per
/// heading, and keeps the TOC data type free of byte-range baggage that
/// its renderer ignores.
pub struct HeadingInfo {
    pub level: HeadingLevel,
    pub text: String,
    pub id: String,
    pub byte_range: Range<usize>,
}

pub struct Stats {
    pub text_buf: String,
    pub heading_count: usize,
    pub code_blocks: Vec<CodeBlockInfo>,
    pub images: Vec<ImageInfo>,
    pub links: Vec<LinkInfo>,
    pub footnote_definitions: Vec<FootnoteDefInfo>,
    pub headings: Vec<HeadingInfo>,
    pub toc_entries: Vec<TocEntry>,
    pub frontmatter: Option<String>,
}

/// Which extract arrays to serialize into the Ruby-side `:extracts` hash.
/// Flags map 1:1 to the Ruby-facing `extract: { ... }` hash keys.
#[derive(Default, Clone, Copy)]
pub struct ExtractFlags {
    pub images: bool,
    pub links: bool,
    pub code_blocks: bool,
    pub headings: bool,
    pub footnote_definitions: bool,
}

impl ExtractFlags {
    pub fn any(&self) -> bool {
        self.images || self.links || self.code_blocks || self.headings || self.footnote_definitions
    }
}

/// Walk events and collect all statistics + TOC entries in one pass.
/// Call BEFORE filters so we measure original content. Each event
/// arrives paired with the byte range of its source span: the Start
/// tag's range is what gets attached to the corresponding extract
/// record.
pub fn collect(events: &[(Event<'_>, Range<usize>)]) -> Stats {
    let mut text_buf = String::new();
    let mut code_blocks: Vec<CodeBlockInfo> = Vec::new();
    let mut images: Vec<ImageInfo> = Vec::new();
    let mut links: Vec<LinkInfo> = Vec::new();
    let mut footnote_definitions: Vec<FootnoteDefInfo> = Vec::new();
    let mut frontmatter: Option<String> = None;
    let mut in_metadata_block = false;

    let mut in_code_block = false;
    let mut in_image = false;
    let mut in_link = false;
    let mut in_footnote_def = false;
    let mut image_alt = String::new();
    let mut link_text = String::new();
    let mut current_code_block: Option<CodeBlockInfo> = None;
    let mut current_image: Option<ImageInfo> = None;
    let mut current_link: Option<LinkInfo> = None;
    let mut current_footnote_def: Option<FootnoteDefInfo> = None;

    let mut toc_entries: Vec<TocEntry> = Vec::new();
    let mut headings: Vec<HeadingInfo> = Vec::new();
    let mut dedup = SlugDeduplicator::new();
    let mut in_heading = false;
    let mut current_heading_level = HeadingLevel::H1;
    let mut current_heading_text = String::new();
    let mut current_heading_range: Range<usize> = 0..0;

    for (event, range) in events {
        match event {
            // Headings
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                current_heading_level = *level;
                current_heading_text.clear();
                current_heading_range = range.clone();
            }
            Event::End(TagEnd::Heading(_)) if in_heading => {
                in_heading = false;
                let base = heading::slugify(&current_heading_text);
                if !base.is_empty() {
                    let slug = dedup.deduplicate(base);
                    toc_entries.push(TocEntry {
                        level: current_heading_level,
                        text: current_heading_text.clone(),
                        slug: slug.clone(),
                    });
                    headings.push(HeadingInfo {
                        level: current_heading_level,
                        text: current_heading_text.clone(),
                        id: slug,
                        byte_range: current_heading_range.clone(),
                    });
                }
            }

            // Frontmatter
            Event::Start(Tag::MetadataBlock(_)) => {
                in_metadata_block = true;
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                in_metadata_block = false;
            }

            // Code blocks
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                current_code_block = Some(CodeBlockInfo {
                    lang,
                    source: String::new(),
                    byte_range: range.clone(),
                });
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if let Some(block) = current_code_block.take() {
                    code_blocks.push(block);
                }
            }

            // Images
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                in_image = true;
                image_alt.clear();
                current_image = Some(ImageInfo {
                    src: dest_url.to_string(),
                    alt: String::new(),
                    title: title.to_string(),
                    byte_range: range.clone(),
                });
            }
            Event::End(TagEnd::Image) => {
                in_image = false;
                if let Some(mut img) = current_image.take() {
                    img.alt = image_alt.clone();
                    images.push(img);
                }
            }

            // Links
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                in_link = true;
                link_text.clear();
                current_link = Some(LinkInfo {
                    href: dest_url.to_string(),
                    text: String::new(),
                    title: title.to_string(),
                    byte_range: range.clone(),
                });
            }
            Event::End(TagEnd::Link) => {
                in_link = false;
                if let Some(mut lnk) = current_link.take() {
                    lnk.text = link_text.clone();
                    links.push(lnk);
                }
            }

            // Footnote definitions
            Event::Start(Tag::FootnoteDefinition(label)) => {
                in_footnote_def = true;
                current_footnote_def = Some(FootnoteDefInfo {
                    label: label.to_string(),
                    text: String::new(),
                    byte_range: range.clone(),
                });
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                in_footnote_def = false;
                if let Some(mut def) = current_footnote_def.take() {
                    // Trim a single trailing space left by our " " separator
                    // after the last text run—makes the captured body
                    // easier to display or diff.
                    if def.text.ends_with(' ') {
                        def.text.pop();
                    }
                    footnote_definitions.push(def);
                }
            }

            // ── Text ──
            Event::Text(t) | Event::Code(t) => {
                if in_metadata_block {
                    // Capture the raw YAML frontmatter text;
                    // frontmatter is structured config, not content.
                    frontmatter = Some(t.to_string());
                } else {
                    // Text inside a code block also contributes to the
                    // document's character/word totals: code is content,
                    // especially for AI/RAG use cases where we want
                    // `word_count` to reflect what an embedding model
                    // would actually see.
                    text_buf.push_str(t);
                    text_buf.push(' ');
                    if in_code_block {
                        if let Some(block) = current_code_block.as_mut() {
                            block.source.push_str(t);
                        }
                    }
                }
                if in_heading {
                    current_heading_text.push_str(t);
                }
                if in_image {
                    image_alt.push_str(t);
                }
                if in_link {
                    link_text.push_str(t);
                }
                if in_footnote_def {
                    if let Some(def) = current_footnote_def.as_mut() {
                        def.text.push_str(t);
                        def.text.push(' ');
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if !in_code_block {
                    text_buf.push(' ');
                }
            }
            _ => {}
        }
    }

    Stats {
        text_buf,
        heading_count: toc_entries.len(),
        code_blocks,
        images,
        links,
        footnote_definitions,
        headings,
        toc_entries,
        frontmatter,
    }
}

/// Build the `:statistics` hash—scalars only.
///
/// When `full` is true (set by `statistics: true`), emits language
/// detection, character/word counts, and every `*_count` field.
/// When false (toc-only mode), emits just `heading_count` so downstream
/// code that relies on it keeps working without upgrading to full stats.
pub fn to_statistics_hash(ruby: &Ruby, stats: &Stats, full: bool) -> Result<RHash, Error> {
    let hash = ruby.hash_new();
    hash.aset(ruby.to_symbol("heading_count"), stats.heading_count)?;

    if full {
        match whatlang::detect(&stats.text_buf) {
            Some(info) => {
                hash.aset(ruby.to_symbol("likely_language"), info.lang().code())?;
                hash.aset(ruby.to_symbol("language_confidence"), info.confidence())?;
            }
            None => {
                hash.aset(ruby.to_symbol("likely_language"), ())?;
                hash.aset(ruby.to_symbol("language_confidence"), ())?;
            }
        }

        hash.aset(
            ruby.to_symbol("character_count"),
            stats.text_buf.trim().chars().count(),
        )?;
        hash.aset(
            ruby.to_symbol("word_count"),
            stats.text_buf.unicode_words().count(),
        )?;
        hash.aset(ruby.to_symbol("code_block_count"), stats.code_blocks.len())?;
        hash.aset(ruby.to_symbol("image_count"), stats.images.len())?;
        hash.aset(ruby.to_symbol("link_count"), stats.links.len())?;
        hash.aset(
            ruby.to_symbol("footnote_definition_count"),
            stats.footnote_definitions.len(),
        )?;
    }

    Ok(hash)
}

/// Build the `:extracts` hash. Only keys whose flag is set appear:
/// callers who opted into one kind aren't charged allocation cost for
/// the others.
pub fn to_extracts_hash(ruby: &Ruby, stats: &Stats, flags: ExtractFlags) -> Result<RHash, Error> {
    let hash = ruby.hash_new();

    if flags.images {
        let arr = ruby.ary_new_capa(stats.images.len());
        for img in &stats.images {
            let h = ruby.hash_new();
            h.aset(ruby.to_symbol("src"), img.src.as_str())?;
            h.aset(ruby.to_symbol("alt"), img.alt.as_str())?;
            h.aset(ruby.to_symbol("title"), img.title.as_str())?;
            h.aset(
                ruby.to_symbol("byte_range"),
                ruby.range_new(img.byte_range.start as i64, img.byte_range.end as i64, true)?,
            )?;
            arr.push(h)?;
        }
        hash.aset(ruby.to_symbol("images"), arr)?;
    }

    if flags.links {
        let arr = ruby.ary_new_capa(stats.links.len());
        for lnk in &stats.links {
            let h = ruby.hash_new();
            h.aset(ruby.to_symbol("href"), lnk.href.as_str())?;
            h.aset(ruby.to_symbol("text"), lnk.text.as_str())?;
            h.aset(ruby.to_symbol("title"), lnk.title.as_str())?;
            h.aset(
                ruby.to_symbol("byte_range"),
                ruby.range_new(lnk.byte_range.start as i64, lnk.byte_range.end as i64, true)?,
            )?;
            arr.push(h)?;
        }
        hash.aset(ruby.to_symbol("links"), arr)?;
    }

    if flags.code_blocks {
        let arr = ruby.ary_new_capa(stats.code_blocks.len());
        for block in &stats.code_blocks {
            let h = ruby.hash_new();
            h.aset(ruby.to_symbol("lang"), block.lang.as_str())?;
            h.aset(ruby.to_symbol("source"), block.source.as_str())?;
            h.aset(
                ruby.to_symbol("byte_range"),
                ruby.range_new(
                    block.byte_range.start as i64,
                    block.byte_range.end as i64,
                    true,
                )?,
            )?;
            arr.push(h)?;
        }
        hash.aset(ruby.to_symbol("code_blocks"), arr)?;
    }

    if flags.headings {
        let arr: RArray = ruby.ary_new_capa(stats.headings.len());
        for entry in &stats.headings {
            let h = ruby.hash_new();
            h.aset(ruby.to_symbol("level"), toc::level_to_u8(entry.level))?;
            h.aset(ruby.to_symbol("text"), entry.text.as_str())?;
            h.aset(ruby.to_symbol("id"), entry.id.as_str())?;
            h.aset(
                ruby.to_symbol("byte_range"),
                ruby.range_new(
                    entry.byte_range.start as i64,
                    entry.byte_range.end as i64,
                    true,
                )?,
            )?;
            arr.push(h)?;
        }
        hash.aset(ruby.to_symbol("headings"), arr)?;
    }

    if flags.footnote_definitions {
        let arr = ruby.ary_new_capa(stats.footnote_definitions.len());
        for def in &stats.footnote_definitions {
            let h = ruby.hash_new();
            h.aset(ruby.to_symbol("label"), def.label.as_str())?;
            h.aset(ruby.to_symbol("text"), def.text.as_str())?;
            h.aset(
                ruby.to_symbol("byte_range"),
                ruby.range_new(def.byte_range.start as i64, def.byte_range.end as i64, true)?,
            )?;
            arr.push(h)?;
        }
        hash.aset(ruby.to_symbol("footnote_definitions"), arr)?;
    }

    Ok(hash)
}
