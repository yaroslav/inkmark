use magnus::{Error, RHash, Ruby};
use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

use crate::autolink;
use crate::emoji;
use crate::heading;
use crate::highlight;
use crate::image;
use crate::link;
use crate::options::{build_options, Flags};
use crate::plain_text;
use crate::scheme_filter::SchemeFilter;
use crate::stats;
use crate::tag_filter;
use crate::toc;

// When `opts_hash` is nil (Ruby passes nil), the caller signals that no
// options have been set—we skip build_options entirely and use hardcoded
// defaults. This eliminates N hash lookups + N symbol creations per render.
pub fn native_to_html(
    ruby: &Ruby,
    source: String,
    opts_hash: Option<RHash>,
) -> Result<String, Error> {
    match opts_hash {
        None => Ok(render_defaults(&source)),
        Some(hash) => {
            let (cm_opts, flags) = build_options(ruby, hash)?;
            Ok(render(&source, cm_opts, flags))
        }
    }
}

pub fn native_to_markdown(
    ruby: &Ruby,
    source: String,
    opts_hash: Option<RHash>,
) -> Result<String, Error> {
    match opts_hash {
        None => Ok(markdown_defaults(&source)),
        Some(hash) => {
            let (cm_opts, flags) = build_options(ruby, hash)?;
            Ok(render_to_markdown(&source, cm_opts, flags))
        }
    }
}

pub fn native_to_plain_text(
    ruby: &Ruby,
    source: String,
    opts_hash: Option<RHash>,
) -> Result<String, Error> {
    match opts_hash {
        None => Ok(plain_text_defaults(&source)),
        Some(hash) => {
            let (cm_opts, flags) = build_options(ruby, hash)?;
            Ok(render_to_plain_text(&source, cm_opts, flags))
        }
    }
}

/// Fast path. Hardcoded-defaults. Matches Inkmark::Options::DEFAULTS exactly:
/// GFM + tables + strikethrough + tasklists + footnotes on, raw HTML
/// suppressed, all allowlists off.
fn render_defaults(source: &str) -> String {
    let mut buf = String::with_capacity(source.len() * 3 / 2);
    let parser = Parser::new_ext(source, default_cm_opts());
    let filtered = parser.map(suppress_raw_html);
    html::push_html(&mut buf, filtered);
    buf
}

/// Same as render_defaults but serializes to Markdown instead of HTML.
fn markdown_defaults(source: &str) -> String {
    let mut buf = String::with_capacity(source.len());
    let parser = Parser::new_ext(source, default_cm_opts());
    let filtered = parser.map(suppress_raw_html);
    cmark_write(filtered, &mut buf);
    buf
}

/// Defaults-only plain-text fast path. Mirrors `markdown_defaults`:
/// same GFM baseline, same raw-HTML suppression.
fn plain_text_defaults(source: &str) -> String {
    let mut buf = String::with_capacity(source.len());
    let parser = Parser::new_ext(source, default_cm_opts());
    let filtered = parser.map(suppress_raw_html);
    plain_text::write_plain_text(filtered, &mut buf);
    buf
}

fn default_cm_opts() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_GFM);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts
}

#[inline]
fn suppress_raw_html(event: Event) -> Event {
    match event {
        Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
        other => other,
    }
}

#[inline]
fn hard_wrap(event: Event) -> Event {
    match event {
        Event::SoftBreak => Event::HardBreak,
        other => other,
    }
}

/// Parse `source` into the content event stream that renderers and chunkers
/// consume.
///
/// YAML frontmatter is removed at this boundary: it is document *metadata*
/// (surfaced via {Inkmark#frontmatter}), never content. pulldown-cmark's HTML
/// renderer ignores metadata blocks and our plain-text writer discards them,
/// but the Markdown serializer (`pulldown-cmark-to-cmark`) faithfully
/// re-emits them as `---\n...\n---`. Rather than re-stripping after the fact
/// in every Markdown/chunk path, we never hand those consumers the events in
/// the first place—so `to_markdown`, `chunks_by_heading`, and
/// `chunks_by_size` are frontmatter-free by construction, with no separate
/// pass and no special-casing of the streaming fast path.
///
/// Frontmatter extraction walks the *raw* parser (see `stats::collect`), so
/// dropping the events here does not affect the `frontmatter` accessor.
pub fn content_events(source: &str, cm_opts: Options) -> impl Iterator<Item = Event<'_>> {
    drop_metadata(Parser::new_ext(source, cm_opts))
}

/// Iterator adapter that filters out `Start(MetadataBlock) … End(MetadataBlock)`
/// runs, including the raw YAML `Text` between the markers. Stateful but
/// composes with both the streaming fast path and the buffered `.collect()`
/// path, so a single definition serves every content consumer.
fn drop_metadata<'a>(events: impl Iterator<Item = Event<'a>>) -> impl Iterator<Item = Event<'a>> {
    let mut in_metadata = false;
    events.filter(move |event| match event {
        Event::Start(Tag::MetadataBlock(_)) => {
            in_metadata = true;
            false
        }
        Event::End(TagEnd::MetadataBlock(_)) => {
            in_metadata = false;
            false
        }
        _ => !in_metadata,
    })
}

/// Full render: parse once, collect stats + TOC from original events,
/// apply filters, render HTML. Returns a Ruby Hash:
///
/// ```ruby
/// { html: "...", toc: "...", toc_html: "...", statistics: {...} }
/// ```
///
/// `statistics: true` implies full stats + TOC. `toc: true` alone gives
/// TOC + a lightweight stats hash (heading_count only). Keys whose
/// feature flag is off are set to nil.
pub fn native_render_full(ruby: &Ruby, source: String, opts_hash: RHash) -> Result<RHash, Error> {
    let (cm_opts, mut flags) = build_options(ruby, opts_hash)?;

    // statistics implies toc + heading_ids
    if flags.statistics {
        flags.toc = true;
        flags.heading_ids = true;
    }

    // Mutual toc / extract[:headings]: one walk powers both surfaces,
    // so enabling either exposes the heading data on both. Keeps users
    // from having to set two flags when the cost is identical.
    if flags.extract.headings {
        flags.toc = true;
    }
    if flags.toc {
        flags.extract.headings = true;
        flags.heading_ids = true;
    }

    // Parse with offset iterator so stats::collect can attach byte
    // ranges to each extract record. The filter pipeline only needs
    // Event values, so we split the tuple into two vecs and drop the
    // ranges before filters run.
    let offset_events: Vec<(Event, std::ops::Range<usize>)> = Parser::new_ext(&source, cm_opts)
        .into_offset_iter()
        .collect();

    // Collect stats/TOC from original events (before filters)
    let collected = stats::collect(&offset_events);

    // Strip ranges, apply filters, render HTML.
    let events: Vec<Event> = offset_events.into_iter().map(|(e, _)| e).collect();
    let events = apply_filters(events, &flags);
    let mut buf = String::with_capacity(source.len() * 3 / 2);
    html::push_html(&mut buf, events.into_iter());

    // Build result hash
    let result = ruby.hash_new();
    result.aset(ruby.to_symbol("html"), buf)?;

    if flags.toc {
        let toc_md = toc::toc_to_markdown(&collected.toc_entries, flags.toc_depth);
        let toc_html_str = toc::toc_to_html(&collected.toc_entries, flags.toc_depth);
        result.aset(ruby.to_symbol("toc"), toc_md)?;
        result.aset(ruby.to_symbol("toc_html"), toc_html_str)?;
    } else {
        result.aset(ruby.to_symbol("toc"), ())?;
        result.aset(ruby.to_symbol("toc_html"), ())?;
    }

    let stats_hash = stats::to_statistics_hash(ruby, &collected, flags.statistics)?;
    result.aset(ruby.to_symbol("statistics"), stats_hash)?;

    // Extracts: present when any extract flag is set (either directly,
    // or implicitly via toc → headings). Nil otherwise so `md.extracts`
    // returns nil for callers who didn't ask.
    if flags.extract.any() {
        let extracts_hash = stats::to_extracts_hash(ruby, &collected, flags.extract)?;
        result.aset(ruby.to_symbol("extracts"), extracts_hash)?;
    } else {
        result.aset(ruby.to_symbol("extracts"), ())?;
    }

    // Frontmatter: raw YAML text extracted from MetadataBlock events.
    // Ruby side parses with YAML.safe_load.
    match &collected.frontmatter {
        Some(fm) => result.aset(ruby.to_symbol("frontmatter"), fm.as_str())?,
        None => result.aset(ruby.to_symbol("frontmatter"), ())?,
    }

    Ok(result)
}

fn render(source: &str, cm_opts: pulldown_cmark::Options, flags: Flags) -> String {
    let mut buf = String::with_capacity(source.len() * 3 / 2);
    let parser = Parser::new_ext(source, cm_opts);

    // Fast path: no buffering filter is active. Stream events straight
    // from the parser through push_html with at most one iterator-level
    // map, zero Vec<Event> allocations. This is the hot path for the
    // default config (only suppress_raw_html is on).
    if !needs_buffer(&flags) {
        html::push_html(&mut buf, parser.map(stream_filter(&flags)));
        return buf;
    }

    let events = apply_filters(parser.collect(), &flags);
    html::push_html(&mut buf, events.into_iter());
    buf
}

fn render_to_markdown(source: &str, cm_opts: pulldown_cmark::Options, flags: Flags) -> String {
    let mut buf = String::with_capacity(source.len());

    // `content_events` strips frontmatter, so the cmark serializer never sees
    // a metadata block to re-emit—on either the streaming or buffered path.
    if !needs_buffer(&flags) {
        cmark_write(
            content_events(source, cm_opts).map(stream_filter(&flags)),
            &mut buf,
        );
        return buf;
    }

    let events = apply_filters(content_events(source, cm_opts).collect(), &flags);
    cmark_write(events.into_iter(), &mut buf);
    buf
}

fn render_to_plain_text(source: &str, cm_opts: pulldown_cmark::Options, flags: Flags) -> String {
    let mut buf = String::with_capacity(source.len());
    let parser = Parser::new_ext(source, cm_opts);

    if !needs_buffer(&flags) {
        plain_text::write_plain_text(parser.map(stream_filter(&flags)), &mut buf);
        return buf;
    }

    let events = apply_filters(parser.collect(), &flags);
    plain_text::write_plain_text(events.into_iter(), &mut buf);
    buf
}

/// Fast-path event mapper. Combines the streaming filters—
/// `suppress_raw_html`, `hard_wrap`, and GFM tagfilter—into one
/// closure so the three render entry points share one implementation.
/// Buffered filters (TOC, allowlists, etc.) go through `apply_filters`
/// instead.
fn stream_filter(flags: &Flags) -> impl Fn(Event) -> Event {
    let shtml = flags.suppress_raw_html;
    let hwrap = flags.hard_wrap;

    // Tagfilter runs only when we're passing raw HTML through AND
    // GFM is active AND the user hasn't opted out. Its output is
    // otherwise wasted work (suppress_raw_html escapes everything).
    let tagf = !flags.suppress_raw_html && flags.gfm && flags.gfm_tag_filter;
    move |e| {
        let e = if tagf { tag_filter::apply_event(e) } else { e };
        let e = if shtml { suppress_raw_html(e) } else { e };
        if hwrap {
            hard_wrap(e)
        } else {
            e
        }
    }
}

/// Returns true when any active filter requires materializing the event stream
/// into a Vec before processing. The fast path avoids this allocation entirely.
fn needs_buffer(flags: &Flags) -> bool {
    flags.heading_ids
        || flags.emoji_shortcodes
        || flags.autolink
        || flags.lazy_images
        || flags.nofollow_external_links
        || flags.syntax_highlight
        || flags.allowed_link_hosts.is_some()
        || flags.allowed_image_hosts.is_some()
        || flags.allowed_link_schemes.is_some()
        || flags.allowed_image_schemes.is_some()
}

/// Apply all active event-level filters to a materialized event Vec.
/// Shared by `render`, `render_to_markdown`, and `native_render_full`.
pub fn apply_filters<'a>(events: Vec<Event<'a>>, flags: &Flags) -> Vec<Event<'a>> {
    let events = apply_pre_handler_filters(events, flags);
    apply_post_handler_filters(events, flags)
}

/// Enrichment filters that run before user handlers:
/// emoji => autolink => heading_ids => suppress_raw_html.
///
/// Handlers see emoji-resolved text, autolinked URLs, and heading IDs
/// already set. Code blocks are still Code events (not yet highlighted).
///
/// Order matters.
pub fn apply_pre_handler_filters<'a>(mut events: Vec<Event<'a>>, flags: &Flags) -> Vec<Event<'a>> {
    // Emoji shortcodes run before heading IDs so a heading like
    // `# :rocket: Launching` generates its slug from the rendered "🚀"
    // rather than from the raw ":rocket:" text.
    if flags.emoji_shortcodes {
        emoji::replace(&mut events);
    }

    // Autolink runs after emoji (so :rocket: is already a char, not a
    // false-positive URL pattern) but before heading_ids (so heading
    // text containing a URL gets that URL linked before the slug is
    // computed). It emits Start(Link)/Text/End(Link), not Event::Html,
    // so it can run before suppress_raw_html safely.
    if flags.autolink {
        events = autolink::autolink(events);
    }

    if flags.heading_ids {
        heading::add_ids(&mut events);
    }

    // GFM tagfilter: escape the nine disallowed tag names in raw HTML.
    // Only runs when raw HTML is actually being passed through—when
    // suppress_raw_html is on, everything becomes escaped text anyway,
    // and running tagfilter first would double-escape via Text events.
    if !flags.suppress_raw_html && flags.gfm && flags.gfm_tag_filter {
        for event in events.iter_mut() {
            if matches!(event, Event::Html(_) | Event::InlineHtml(_)) {
                let taken = std::mem::replace(event, Event::SoftBreak);
                *event = tag_filter::apply_event(taken);
            }
        }
    }

    if flags.suppress_raw_html {
        for event in events.iter_mut() {
            match event {
                Event::Html(_) | Event::InlineHtml(_) => {
                    let taken = std::mem::replace(event, Event::SoftBreak);
                    match taken {
                        Event::Html(h) | Event::InlineHtml(h) => *event = Event::Text(h),
                        _ => unreachable!(),
                    }
                }
                _ => {}
            }
        }
    }

    if flags.hard_wrap {
        for event in events.iter_mut() {
            if matches!(event, Event::SoftBreak) {
                *event = Event::HardBreak;
            }
        }
    }

    events
}

/// HTML-emitting filters that run after user handlers:
/// syntax_highlight => allowlists => lazy_images => nofollow.
///
/// Accepts `Vec<Event<'static>>` so it can be called on the owned events
/// produced by the handler tree after serialization.
///
/// Order matters.
pub fn apply_post_handler_filters<'a>(mut events: Vec<Event<'a>>, flags: &Flags) -> Vec<Event<'a>> {
    // The filters below all synthesize Event::Html and must run after
    // raw-HTML suppression (done in pre_handler_filters). Suppress_raw_html
    // rewrites every Event::Html to Event::Text, which would HTML-escape
    // our injected tags into visible angle brackets.
    if flags.syntax_highlight {
        events = highlight::highlight(events);
    }

    // Host and scheme allowlists must run before the Html-emitting
    // filters below, because those collapse Start/End(Link) and
    // Start/End(Image) into single Event::Html events—after which
    // the allowlist can no longer see the dest_url on a structured
    // Link/Image tag.
    if let Some(set) = &flags.allowed_link_hosts {
        events = link::filter_by_hosts(events, set);
    }
    if let Some(set) = &flags.allowed_image_hosts {
        events = image::filter_by_hosts(events, set);
    }

    // Fuse both scheme filters into a single SchemeFilter pass—handles
    // link and image events in one walk of the stream.
    if flags.allowed_link_schemes.is_some() || flags.allowed_image_schemes.is_some() {
        events = SchemeFilter::new(
            events.into_iter(),
            flags.allowed_link_schemes.as_deref(),
            flags.allowed_image_schemes.as_deref(),
        )
        .collect();
    }
    if flags.lazy_images {
        events = image::add_lazy_loading(events);
    }
    if flags.nofollow_external_links {
        events = link::add_nofollow(events);
    }

    events
}

fn cmark_write<'a, I: Iterator<Item = Event<'a>>>(events: I, buf: &mut String) {
    pulldown_cmark_to_cmark::cmark(events, buf).expect("markdown serialization failed");
}
