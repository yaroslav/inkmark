use globset::{Glob, GlobSet, GlobSetBuilder};
use magnus::value::{Id, LazyId};
use magnus::{Error, RHash, Ruby};
use pulldown_cmark::Options;

use crate::stats::ExtractFlags;

// `sym_id!(ruby, "name")` resolves a Ruby option-key symbol through
// a block-scoped `static LazyId` cache. Each call site expands to
// its own static, so the intern happens exactly once per key over
// the process's lifetime; subsequent calls return the cached `Id`
// directly. Avoids the `ruby.to_symbol(key)` intern-table lookup
// that would otherwise run on every render for 25+ keys and kill
// performance.
macro_rules! sym_id {
    ($ruby:expr, $name:literal) => {{
        static K: LazyId = LazyId::new($name);
        LazyId::get_inner_with(&K, $ruby)
    }};
}

/// Runtime flags that don't map to pulldown-cmark's `Options` bitflags but
/// instead drive Inkmark's own event filters (raw-HTML suppression, heading-id
/// generation, and future filters). Grouped into a struct so `build_options`
/// stays single-return as we add more filter knobs.
pub struct Flags {
    pub suppress_raw_html: bool,
    pub hard_wrap: bool,
    pub gfm: bool,
    pub gfm_tag_filter: bool,
    pub heading_ids: bool,
    pub emoji_shortcodes: bool,
    pub autolink: bool,
    pub lazy_images: bool,
    pub nofollow_external_links: bool,
    pub syntax_highlight: bool,
    pub toc: bool,
    pub toc_depth: Option<u8>,
    pub statistics: bool,
    // Extract-array flags, parsed from the nested `extract: {...}` hash.
    // `ExtractFlags::any()` tells the renderer whether to take the
    // single-pass stats/extract path.
    pub extract: ExtractFlags,
    // Compiled host-glob allowlists. `None` means the option was unset
    // (no filtering); `Some(set)` means filter: `set` may be empty, in
    // which case nothing matches and every external link/image is
    // rejected.
    pub allowed_link_hosts: Option<GlobSet>,
    pub allowed_image_hosts: Option<GlobSet>,
    // URL scheme allowlists for markdown-emitted links/images. `None`
    // means the option is unset (filtering disabled—the Ruby-side
    // default); `Some(list)` means filter. Stored as Vec rather than
    // HashSet because realistic scheme lists are 2–5 entries, where a
    // linear scan beats a hash table on cache alone.
    pub allowed_link_schemes: Option<Vec<String>>,
    pub allowed_image_schemes: Option<Vec<String>>,
}

pub fn build_options(ruby: &Ruby, hash: RHash) -> Result<(Options, Flags), Error> {
    let mut opts = Options::empty();

    let get_bool = |id: Id| -> Result<bool, Error> {
        let value: Option<bool> = hash.lookup(id)?;
        Ok(value.unwrap_or(false))
    };

    // Pull each bool option once; "gfm" used to feed both `opts` and
    // `flags` via a redundant second lookup—now read once, reused.
    let gfm = get_bool(sym_id!(ruby, "gfm"))?;
    let tables = get_bool(sym_id!(ruby, "tables"))?;
    let strikethrough = get_bool(sym_id!(ruby, "strikethrough"))?;
    let tasklists = get_bool(sym_id!(ruby, "tasklists"))?;
    let footnotes = get_bool(sym_id!(ruby, "footnotes"))?;
    let smart_punctuation = get_bool(sym_id!(ruby, "smart_punctuation"))?;
    let heading_attributes = get_bool(sym_id!(ruby, "heading_attributes"))?;
    let math = get_bool(sym_id!(ruby, "math"))?;
    let definition_list = get_bool(sym_id!(ruby, "definition_list"))?;
    let superscript = get_bool(sym_id!(ruby, "superscript"))?;
    let subscript = get_bool(sym_id!(ruby, "subscript"))?;
    let wikilinks = get_bool(sym_id!(ruby, "wikilinks"))?;
    let frontmatter = get_bool(sym_id!(ruby, "frontmatter"))?;

    if gfm {
        opts.insert(Options::ENABLE_GFM);
    }
    if tables {
        opts.insert(Options::ENABLE_TABLES);
    }
    if strikethrough {
        opts.insert(Options::ENABLE_STRIKETHROUGH);
    }
    if tasklists {
        opts.insert(Options::ENABLE_TASKLISTS);
    }
    if footnotes {
        opts.insert(Options::ENABLE_FOOTNOTES);
    }
    if smart_punctuation {
        opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    }
    if heading_attributes {
        opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    }
    if math {
        opts.insert(Options::ENABLE_MATH);
    }
    if definition_list {
        opts.insert(Options::ENABLE_DEFINITION_LIST);
    }
    if superscript {
        opts.insert(Options::ENABLE_SUPERSCRIPT);
    }
    if subscript {
        opts.insert(Options::ENABLE_SUBSCRIPT);
    }
    if wikilinks {
        opts.insert(Options::ENABLE_WIKILINKS);
    }
    if frontmatter {
        opts.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    }

    let flags = Flags {
        suppress_raw_html: !get_bool(sym_id!(ruby, "raw_html"))?,
        hard_wrap: get_bool(sym_id!(ruby, "hard_wrap"))?,
        gfm,
        gfm_tag_filter: get_bool(sym_id!(ruby, "gfm_tag_filter"))?,
        heading_ids: get_bool(sym_id!(ruby, "heading_ids"))?,
        emoji_shortcodes: get_bool(sym_id!(ruby, "emoji_shortcodes"))?,
        autolink: get_bool(sym_id!(ruby, "autolink"))?,
        lazy_images: get_bool(sym_id!(ruby, "lazy_images"))?,
        nofollow_external_links: get_bool(sym_id!(ruby, "nofollow_external_links"))?,
        syntax_highlight: get_bool(sym_id!(ruby, "syntax_highlight"))?,
        toc: get_bool(sym_id!(ruby, "toc"))?,
        toc_depth: hash.lookup::<_, Option<u8>>(sym_id!(ruby, "toc_depth"))?,
        statistics: get_bool(sym_id!(ruby, "statistics"))?,
        extract: build_extract_flags(ruby, &hash)?,
        allowed_link_hosts: build_host_globset(
            ruby,
            &hash,
            sym_id!(ruby, "allowed_link_hosts"),
            "allowed_link_hosts",
        )?,
        allowed_image_hosts: build_host_globset(
            ruby,
            &hash,
            sym_id!(ruby, "allowed_image_hosts"),
            "allowed_image_hosts",
        )?,
        allowed_link_schemes: build_scheme_set(&hash, sym_id!(ruby, "allowed_link_schemes"))?,
        allowed_image_schemes: build_scheme_set(&hash, sym_id!(ruby, "allowed_image_schemes"))?,
    };
    Ok((opts, flags))
}

/// Read an optional `Array<String>` option and compile it into a `GlobSet`.
/// Returns `Ok(None)` when the option is `nil` (the Ruby-side default) —
/// this signals "filtering disabled" to the event pipeline.
///
/// An empty array compiles to an empty `GlobSet` that matches nothing, so
/// `allowed_link_hosts: []` acts as a deny-all allowlist. Pattern compile
/// failures surface as a Ruby `ArgumentError` with the bad pattern quoted
/// so the user can find and fix it.
fn build_host_globset(
    ruby: &Ruby,
    hash: &RHash,
    key_id: Id,
    key_name: &str,
) -> Result<Option<GlobSet>, Error> {
    let patterns: Option<Vec<String>> = hash.lookup(key_id)?;
    let Some(patterns) = patterns else {
        return Ok(None);
    };

    let mut builder = GlobSetBuilder::new();
    for pattern in &patterns {
        let glob = Glob::new(pattern).map_err(|e| {
            Error::new(
                ruby.exception_arg_error(),
                format!("invalid glob pattern in {key_name}: {pattern:?}—{e}"),
            )
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|e| {
        Error::new(
            ruby.exception_arg_error(),
            format!("failed to compile {key_name} globset: {e}"),
        )
    })?;
    Ok(Some(set))
}

/// Read an optional `Array<String>` scheme allowlist and normalize to
/// lowercase. Returns `Ok(None)` when the option is `nil`, signalling
/// "filtering disabled" to the pipeline. An empty array compiles to an
/// empty `Vec` that matches nothing, which blocks every absolute URL
/// (relative URLs still pass through
/// [`crate::url_match::is_scheme_allowed`]).
fn build_scheme_set(hash: &RHash, key_id: Id) -> Result<Option<Vec<String>>, Error> {
    let schemes: Option<Vec<String>> = hash.lookup(key_id)?;
    Ok(schemes.map(|list| list.into_iter().map(|s| s.to_ascii_lowercase()).collect()))
}

/// Read the nested `extract: { images: true, ... }` hash and compile to
/// an `ExtractFlags`. Nil / missing option → all flags off.
///
/// Ruby-side validation (`Inkmark::Options`) enforces the key set and
/// boolean value type, so by the time we get here an unknown key or
/// non-boolean value has already raised `ArgumentError`. We still read
/// defensively using `Option<bool>` + `unwrap_or(false)` so that a
/// missing sub-key is treated as "off".
fn build_extract_flags(ruby: &Ruby, hash: &RHash) -> Result<ExtractFlags, Error> {
    let nested: Option<RHash> = hash.lookup(sym_id!(ruby, "extract"))?;
    let Some(nested) = nested else {
        return Ok(ExtractFlags::default());
    };

    let read = |id: Id| -> Result<bool, Error> {
        let v: Option<bool> = nested.lookup(id)?;
        Ok(v.unwrap_or(false))
    };

    Ok(ExtractFlags {
        images: read(sym_id!(ruby, "images"))?,
        links: read(sym_id!(ruby, "links"))?,
        code_blocks: read(sym_id!(ruby, "code_blocks"))?,
        headings: read(sym_id!(ruby, "headings"))?,
        footnote_definitions: read(sym_id!(ruby, "footnote_definitions"))?,
    })
}
