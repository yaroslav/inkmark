# Inkmark

A very fast, feature-packed, AI-first markdown gem for Ruby.

[![GitHub Release](https://img.shields.io/github/v/release/yaroslav/inkmark)](https://github.com/yaroslav/inkmark/releases)
[![Docs](https://img.shields.io/badge/yard-docs-blue.svg)](https://rubydoc.info/gems/inkmark)

<div align="center">
  <img src="https://raw.githubusercontent.com/yaroslav/inkmark/refs/heads/main/assets/images/inky.png" width="400" height="400" alt="Inky">
</div>

- **Very fast**. Up to 1.3× faster than redcarpet _(not CommonMark-conformant)_, about 3×–9× faster than other Ruby Markdown gems with native extensions. Built with Rust, based on [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark), uses SIMD.
- **No surprises**. CommonMark + GitHub Flavored Markdown conformance.
- **"Batteries included" approach**. Build lots of useful features, make them easy to use and as fast as possible.
- **Easy to use**. As simple as a one-method API. Pass options inline as a hash, set them one by one, or set default options for the entire application. 
- **Feature-packed**. Server-side syntax highlighting with themes, frontmatter support, table of contents in Markdown and HTML, plain text export, extraction of headers/links/images, statistics (character and word count, likely document language, blocks count), lazy image loading attributes, emoji shortcodes, autolinks, heading IDs with Unicode-transliterated slugs, wikilinks, footnotes, tables, task lists, smart punctuation, hard wraps, "nofollow/noopener" on external links.
- **AI-first**. Two chunking primitives: heading-based with breadcrumbs and per-chunk character/word counts, and sliding-window with overlap for size-bounded chunks where headings are absent or uneven. Block-aware or word-aware truncation for context-window budgeting. Markdown-to-Markdown pipeline. Plain-text extraction for embedding models. Structured extraction of headings, images, links, code blocks—each carrying byte ranges back into the source.
- **Security conscious**. Raw HTML denied by default. Hostname and URL-scheme allowlists for both links and images. GFM tagfilter for dangerous tags. A Rust-backed gem.
- **Easy extension API**. Hook any element with a Ruby block—no subclassing, no intermediate AST, no HTML post-processing. Rewrite URLs, swap code blocks for your own renderer, drop subtrees, or just walk the document for analysis. Handlers fire inside the single-pass parser, so extension costs essentially nothing beyond the render itself—and far less than regexing over output HTML.

## Contents

- [Installation](#installation)
- [Quick start](#quick-start)
- [Presets](#presets)
- [Options](#options)
- [Raw HTML](#raw-html)
- [Host allowlists](#host-allowlists)
- [URL scheme filtering](#url-scheme-filtering)
- [Statistics and extraction](#statistics-and-extraction)
- [Chunks extraction (for RAG)](#chunks-extraction-for-rag)
- [Truncation](#truncation)
- [Plain-text extraction](#plain-text-extraction)
- [Markdown-to-Markdown pipeline](#markdown-to-markdown-pipeline)
- [Event handlers](#event-handlers)
- [Benchmarks](#benchmarks)
- [Contributing](#contributing)
- [Acknowledgements](#acknowledgements)
- [License](#license)

## Installation

    bundle add inkmark

Or in your `Gemfile`:

```ruby
gem "inkmark"
```

Ruby 3.3+ is supported.

## Quick start

```ruby
require "inkmark"

# Class-method shortcut
Inkmark.to_html("**hello**")
# => "<p><strong>hello</strong></p>\n"

# Instance form
Inkmark.new("# Hello").to_html

# With options
Inkmark.to_html("hi <em>there</em>", options: { raw_html: true })

# Mutable options via accessor
g = Inkmark.new("# Table\n\n| a | b |\n|---|---|\n| 1 | 2 |")
g.options.tables = false
g.to_html  # tables render as paragraphs now
```

## Presets

Inkmark ships presets as opinionated shortcuts for common
rendering profiles. Pass one via `preset:` in the options hash; every
other option in the hash overrides the preset's values (deep-merging
for nested element-policy hashes). You can—and are recommended to!—override preset options as you see fit.

- **`:recommended`**: a curated profile for modern web content. On
  top of GFM, enables smart punctuation, auto heading IDs, lazy-loading
  images with an `http`/`https` scheme allowlist, autolinks,
  `rel="nofollow noopener"` on external links, a scheme allowlist for
  link destinations, emoji shortcodes, syntax highlighting, hard wraps,
  and frontmatter parsing.

  **This is a good starting point for most apps**. Still, you are expected to
  override individual options to match your specific needs (e.g. adding statistics and table of contents, tightening link/image allowlists to your own hostnames, turning off features you don't want).

- **`:trusted`**: `:recommended` plus raw HTML pass-through.
  **Dangerous.** Intended only for content you fully trust: internal,
  team-authored. With raw HTML on, Inkmark does no sanitization beyond
  the narrow GFM tagfilter (turn it off on your own risk); the caller is
  responsible for output safety. Do not apply this preset to anything a user can influence, directly or indirectly.

- **`:gfm`**: the bare default. CommonMark plus the core GFM extensions
  (tables, strikethrough, tasklists, footnotes, tagfilter). Strict,
  conservative, and matches the render profile of every other major
  GFM engine. Everything else is off.

- **`:commonmark`**: the minimum. Strict CommonMark. No GFM extensions, no
  typographics, nothing opinionated.

```ruby
# Recommended profile
Inkmark.to_html(md, options: { preset: :recommended })

# Recommended profile with stats and table of contents
Inkmark.to_html(md, options: { preset: :recommended, statistics: true, toc: true })

# Recommended profile, but disable smart punctuation
Inkmark.to_html(md, options: { preset: :recommended, smart_punctuation: false })

# Just GFM (the default)
Inkmark.to_html(md)
Inkmark.to_html(md, options: { preset: :gfm })     # equivalent

# Recommended profile with a tightened link-host allowlist
Inkmark.to_html(md, options: {
  preset: :recommended,
  links:  { allowed_hosts: ["*.example.com"] }
})

# Trusted content (raw HTML passes through—use with care)
Inkmark.to_html(internal_doc, options: { preset: :trusted })
```

## Options

GFM extensions are on by default; raw HTML rendering is off by default.
Pass a hash to `Inkmark.to_html` / `Inkmark.new`, or mutate a `Inkmark::Options`
instance via its accessors.

| Key | Default | Description |
|---|---|---|
| `gfm` | `true` | GFM conformance mode + tables, strikethrough, tasklists, and footnotes. |
| `gfm_tag_filter` | `true` | GFM "Disallowed Raw HTML" extension. When `gfm` and `raw_html` are both true, protects you from several predefined tags (`title`, `textarea`, `style`, `xmp`, `iframe`, `noembed`, `noframes`, `script`, `plaintext`). No effect when `raw_html: false`. |
| `tables` | `true` | GFM pipe tables with optional column alignment markers (`:---`, `:---:`, `---:`). |
| `strikethrough` | `true` | `~~text~~` renders as `<del>text</del>`. |
| `tasklists` | `true` | `- [ ]` and `- [x]` render as disabled checkboxes. |
| `footnotes` | `true` | `text[^1]` + `[^1]: body` renders as superscript links and footnote block. |
| `raw_html` | `false` | Pass raw HTML through unescaped. Off by default for untrusted-input safety. **When enabled, the caller is fully responsible for sanitizing the output—see the [Raw HTML](#raw-html) section.** |
| `smart_punctuation` | `false` | Convert `"..."` → `"..."`, `...` → `…`, `--` → `–`, `---` → `—`. |
| `headings` | `{ attributes: false, ids: false }` | Heading-related policy. `:attributes` enables `# Heading {#id .klass}` Markdown inline attribute syntax; `:ids` auto-generates `id="slug"` on every heading from its text, with automatic Unicode transliteration of non-English headings (duplicates get a counter suffix; user-supplied ids from `:attributes` win). Deep-merges over defaults—pass only the sub-keys you care about. |
| `images` | `{ lazy: false, allowed_hosts: nil, allowed_schemes: nil }` | Image-related policy. `:lazy` adds `loading="lazy" decoding="async"` to every `<img>`. `:allowed_hosts` is a glob allowlist for `<img src>` hostnames (see examples; non-matching images drop to alt text). `:allowed_schemes` is a URL-scheme allowlist—typical: `["http", "https"]` to block `data:` image URIs. Both allowlists default to `nil` (no filtering); `[]` deny-all-external. Deep-merges. |
| `links` | `{ autolink: false, nofollow: false, allowed_hosts: nil, allowed_schemes: nil }` | Link-related policy. `:autolink` auto-links bare URLs and emails with correct boundary detection. `:nofollow` adds `rel="nofollow noopener"` to external `<a>` tags. `:allowed_hosts` / `:allowed_schemes` are glob / scheme allowlists for `<a href>` (relative/anchor/mailto URLs are never filtered). Non-matching links unwrap to plain text. Deep-merges. |
| `emoji_shortcodes` | `false` | Replace gemoji-style `:shortcode:` sequences with their emoji character (`:rocket:` → 🚀). Unknown codes and codes inside code blocks are preserved. |
| `syntax_highlight` | `false` | Server-side syntax highlighting for fenced code blocks with a language tag. Uses the `syntect` Rust crate with CSS class output. Batteries included: pair with CSS from `Inkmark.highlight_css` for the theme stylesheet. |
| `hard_wrap` | `false` | Treat every single newline as a hard line break (`<br />`). By default a bare `\n` is a soft break rendered as a space. Enable for one-sentence-per-line content or when migrating from renderers that default to hard wraps. |
| `toc` | `false` | Collect a table of contents from headings. Accepts `true` / `false` for simple enable/disable, or a Hash like `toc: { depth: 3 }` to limit which heading levels appear in the rendered TOC (h1–h3 in that example; default is no limit). Enables `Inkmark#toc` which returns a `Inkmark::Toc` value object (`#to_markdown` / `#to_html` / `#to_s`). Implicitly enables `headings: { ids: true }`. Also populates a lightweight `Inkmark#statistics` with `heading_count`. Depth affects only the rendered TOC; `heading_count`, `extracts[:headings]`, and `chunks_by_heading` still see every heading. |
| `statistics` | `false` | Collect scalar document statistics during parsing: language detection, character/word counts, and `*_count` fields for headings, code blocks, images, links, and footnote definitions. See examples. For structured arrays of records, use `extract`. Implies `toc` and `headings: { ids: true }`. |
| `extract` | `nil` | Hash opting into structured extraction of specific element kinds. Keys: `:images`, `:links`, `:code_blocks`, `:headings`, `:footnote_definitions`—each `true`/`false`. When set, `Inkmark#extracts` returns a Hash keyed by the requested kinds, each with an Array of record Hashes including a `:byte_range`. `extract: { headings: true }` and `toc: true` trigger each other—one heading walk powers both surfaces. |
| `math` | `false` | Recognize `$inline$` and `$$display$$` math blocks. |
| `definition_list` | `false` | `term\n: definition` renders as `<dl>`. |
| `superscript` | `false` | `^text^` renders as `<sup>`. |
| `subscript` | `false` | `~text~` renders as `<sub>`. Conflicts with strikethrough—enable only one. |
| `wikilinks` | `false` | `[[Page]]` and `[[Page\|label]]` render as links. |
| `frontmatter` | `false` | Frontmatter (YAML metadata at the start of the document). Parsed and exposed via `Inkmark#frontmatter`; the block is stripped from rendered output. |

Options can be supplied either way:

```ruby
# As a hash at construction
Inkmark.to_html(md, options: { math: true, tables: false })

# Via mutable accessor
g = Inkmark.new(md)
g.options.math = true
g.options.tables = false
g.to_html

# Process-level defaults, to set in your application initializer
Inkmark.default_options.math = true
Inkmark.new(md).to_html  # picks up the default
```

Unknown option keys raise `ArgumentError` immediately, including via the
hash form—typos fail loudly:

```ruby
Inkmark.new("x", options: { taples: true })
# => ArgumentError: unknown Inkmark option: :taples
```

## Raw HTML

Raw HTML is suppressed by default. This is safe-by-default for rendering untrusted markdown:

```ruby
Inkmark.to_html("<script>alert(1)</script>")
# => "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>\n"
```

Enable pass-through with `raw_html: true`; _only do this for trusted
input_:

```ruby
Inkmark.to_html("<em>keep me</em>", options: { raw_html: true })
# => "<p><em>keep me</em></p>\n"
```

> **Your responsibility.** With `raw_html: true` you are fully
> responsible for every `<tag>` that reaches the HTML output. Inkmark does not
> sanitize raw HTML beyond the narrow GFM tagfilter described below—it will
> happily emit `<img onerror="…">`, `<a href="javascript:…">`, `<style>`
> contents, and any other attack surface the source contains. Always pipe the
> output through a dedicated sanitizer (like [Loofah][] or
> [rails-html-sanitizer][]) before rendering untrusted content in a page.

[Loofah]: https://github.com/flavorjones/loofah
[rails-html-sanitizer]: https://github.com/rails/rails-html-sanitizer

Even with `raw_html: true`, the **GFM tagfilter** stays on by
default and escapes nine unsafe tag names—`title`, `textarea`, `style`, `xmp`,
`iframe`, `noembed`, `noframes`, `script`, `plaintext`. This is required for GFM conformance. Opt out with `gfm_tag_filter: false` (or `gfm: false`) if you need raw pass-through of those tags—trusted input only. The tagfilter is a narrow spec-compliance pass, **not** a sanitizer—the responsibility note above still applies in full.

```ruby
Inkmark.to_html("<script>alert(1)</script>", options: { raw_html: true })
# => "<p>&lt;script>alert(1)&lt;/script></p>\n"
```

## Host allowlists

Restrict which hostnames can appear in links and images by passing glob
patterns. Disallowed links have their `<a>` tags stripped (the link text
stays); disallowed images drop to their alt text (or disappear when alt
is empty). Relative URLs, anchors, `mailto:`, and other non-web schemes
pass through unchanged—only `http://` / `https://` URLs are matched.

```ruby
Inkmark.to_html(md, options: {
  links:  { allowed_hosts: ["example.com", "*.example.com"] },
  images: { allowed_hosts: ["{cdn,static,img}.example.com"] }
})
```

Patterns use glob syntax (same engine as `.gitignore`), **not regex**:

- `example.com`: exact host only
- `*.example.com`: any subdomain (matches `cdn.example.com`, `a.b.example.com`; does **not** match bare `example.com`)
- `{cdn,static}.example.com`: brace alternation for multiple explicit hosts
- `*.{example,trusted}.com`: combine wildcards and alternation

Hostnames are matched case-insensitively and ports are ignored. An empty
array `[]` blocks every external link or image while still allowing
relative URLs.

## URL scheme filtering

For rendering untrusted markdown, opt in to scheme allowlists to block
`javascript:`, `data:`, and other dangerous URL schemes in links and
images:

```ruby
Inkmark.to_html(md, options: {
  links:  { allowed_schemes: ["http", "https", "mailto"] },
  images: { allowed_schemes: ["http", "https"] }
})
```

Disallowed links are unwrapped (text stays, `<a>` tags drop); disallowed
images drop to alt text. Relative paths, anchors, and protocol-relative
URLs pass through—no scheme to check.

```ruby
opts = { links: { allowed_schemes: ["http", "https"] } }

Inkmark.to_html("[click](javascript:alert(1))", options: opts)
# => "<p>click</p>\n"

Inkmark.to_html("![pic](data:image/svg+xml,<svg/onload=evil()>)",
               options: { images: { allowed_schemes: ["http", "https"] } })
# => "<p>pic</p>\n"   # dropped to alt text
```

**Scope:** scheme filtering applies to markdown-emitted links and images
(`[text](url)` / `![alt](url)`). Raw HTML `<a href>` / `<img src>` inside
`raw_html: true` content is *not* filtered—for that case use a
downstream HTML sanitizer like Loofah.

**Default:** filtering is off. Full CommonMark autolink conformance is
preserved (including uncommon schemes like `irc:` and `ftp:`). Add the
filter explicitly when rendering untrusted input.

## Statistics and extraction

Inkmark collects document metadata as a side effect of the single render pass.
Two independent options control what's exposed:

- **`statistics: true`** populates `Inkmark#statistics` with scalar counts and
  language detection—nothing you have to iterate.
- **`extract: { kind: true, ... }`** populates `Inkmark#extracts` with structured
  arrays of records. Opt into only the kinds you need; unasked-for arrays are
  never allocated.

```ruby
md = Inkmark.new(source, options: {
  statistics: true,
  extract: {
    images:               true,
    links:                true,
    code_blocks:          true,
    headings:             true,
    footnote_definitions: true
  }
})
md.to_html

md.statistics
# => {
#      heading_count:             2,
#      likely_language:           "eng",
#      language_confidence:       0.93,
#      character_count:           142,
#      word_count:                28,
#      code_block_count:          1,
#      image_count:               1,
#      link_count:                2,
#      footnote_definition_count: 1,
#    }

md.extracts[:code_blocks]
# => [{ lang: "ruby", source: "puts \"hello\"\n", byte_range: 78...101 }]

md.extracts[:headings]
# => [
#      { level: 1, text: "Hello World",  id: "hello-world",  byte_range: 0...14 },
#      { level: 2, text: "Code Example", id: "code-example", byte_range: 68...83 }
#    ]
```

### Extract record shapes

| Kind                      | Fields                                         |
|---------------------------|------------------------------------------------|
| `:images`                 | `src`, `alt`, `title`, `byte_range`            |
| `:links`                  | `href`, `text`, `title`, `byte_range`          |
| `:code_blocks`            | `lang`, `source`, `byte_range`                 |
| `:headings`               | `level`, `text`, `id`, `byte_range`            |
| `:footnote_definitions`   | `label`, `text`, `byte_range`                  |

`byte_range` is an exclusive `Range` (`start...end`) pointing into the original
source string—slice with `source.byteslice(r.begin, r.size)` to recover the
raw Markdown. `source` on `:code_blocks` is pulldown-cmark's pre-filter code
content, so enabling `syntax_highlight: true` does not mutate it.

### Mutual trigger: `toc` ↔ `extract[:headings]`

One heading walk powers both the TOC renderer and the heading extract, so the
two options trigger each other. Enabling either gives you access to both
`Inkmark#toc` (with `#to_markdown` / `#to_html`) and `Inkmark#extracts[:headings]`.

```ruby
Inkmark.new(source, options: { toc: true }).extracts[:headings]
# => [{ level: 1, text: "Hello World", id: "hello-world", byte_range: 0...14 }, ...]
```

## Chunks extraction (for RAG)

`Inkmark.chunks_by_heading` splits a document by heading into an ordered
Array of section Hashes. Each section's `:content` is **filter-applied
Markdown**—emoji expanded, URLs autolinked, allowlists applied—serialized
back through pulldown-cmark. Designed as the first stage of a
chunk → embed → retrieve pipeline.

```ruby
sections = Inkmark.chunks_by_heading(readme)
sections.each do |s|
  puts "#{'#' * s[:level]} #{s[:heading]} (#{s[:id]})"
  puts s[:content]
end
```

Each entry:

```ruby
{
  heading:    "From source",           # String, or nil for the preamble
  level:      3,                       # 1-6, or 0 for the preamble
  id:         "from-source",           # slug, or nil for the preamble
  breadcrumb: ["Docs", "Installation"], # ancestor heading texts, root to parent
  content:    "Run `bundle install`...\n"  # filter-applied Markdown
}
```

Sections are **hierarchical**: a `##` section's `:content` includes any
nested `###` subsections, which also appear as their own entries. Content
before the first heading (if any) becomes a preamble entry with
`heading: nil` and `level: 0`.

`:breadcrumb` carries the ancestor heading texts from root to immediate
parent. Root-level sections and the preamble have an empty array. Skipped
levels are omitted, so an `###` directly under an `#` has `breadcrumb:
["Top"]`, not `["Top", nil]`. RAG pipelines typically prepend the
breadcrumb to each chunk before embedding—it gives the vector model a
cheap signal about the chunk's place in the document:

Enable `statistics: true` to add `:character_count` and `:word_count` to
every section entry. Counts reflect the section's filter-applied text
content including any code-block bodies (code is content for embedding
purposes, not just prose). Numbers across sections won't sum to the
document total because sections overlap hierarchically—a parent section's
count includes its nested subsections.

```ruby
Inkmark.chunks_by_heading(doc, options: {statistics: true})
# => [
#   { heading: "Installation", level: 2, id: "installation",
#     breadcrumb: ["Intro"],
#     character_count: 180, word_count: 32,
#     content: "..." },
#   ...
# ]
```

```ruby
Inkmark.chunks_by_heading(readme).each do |s|
  next if s[:heading].nil?  # skip preamble
  context = (s[:breadcrumb] + [s[:heading]]).join(" > ")
  embed_and_store("#{context}\n\n#{s[:content]}", metadata: {id: s[:id]})
end
```

### Picking specific sections

`chunks_by_heading` always returns the full array. Use plain `Enumerable`
to slice it however you need:

```ruby
sections = Inkmark.chunks_by_heading(readme)

# Find one by heading text
sections.find { |s| s[:heading] == "Installation" }

# Filter by regexp
sections.select { |s| s[:heading]&.match?(/install|usage/i) }

# All top-level headings only
sections.select { |s| s[:level] == 1 }

# Skip the preamble
sections.reject { |s| s[:heading].nil? }
```

No filter kwarg on the method—`.select` / `.find` / `.reject` already
cover every filtering shape, and you can compose conditions freely
(heading AND level, or heading NOT in a blocklist, etc.). The preamble
is a regular entry with `heading: nil` and falls out of Regexp/String
filters naturally (`nil == "Foo"` is false; `nil&.match?(x)` is nil).

### RAG pipeline caveat: HTML-emitting filters

**Disable `syntax_highlight`, `images: { lazy: true }`, and `links: { nofollow: true }`
when chunking for RAG.** These filters embed raw `<pre>…`, `<img loading=…>`,
and `<a rel=…>` HTML into the serialized Markdown; the HTML noise hurts
embedding quality for downstream semantic search.

```ruby
sections = Inkmark.chunks_by_heading(doc, options: {
  emoji_shortcodes: true,    # keep—improves semantic signal
  links: {
    autolink:        true,                # keep—proper anchor markdown
    allowed_schemes: %w[http https mailto], # keep—safe URLs
    nofollow:        false                 # off—would embed <a rel=...> HTML
  },
  images: { lazy: false },   # off—would embed <img loading=...> HTML
  syntax_highlight: false    # off—would embed <pre><span...> HTML
})
```

### Scope

`chunks_by_heading` is a **structural chunking primitive**, not a
complete RAG chunker. It splits a document along heading boundaries.
For documents without headings—or when you need a strict size
budget regardless of document structure—reach for
[`chunks_by_size`](#sliding-window-chunking) below.

Inkmark does not ship token-based budgeting (there is no embedded
tokenizer). Use `character_count` / `word_count` or your own tokenizer
to approximate. Prepending document titles or parent-heading
breadcrumbs to each chunk is a few lines of Ruby on top of the array
this method returns.

### Sliding-window chunking

`Inkmark.chunks_by_size` splits a document into fixed-size chunks with
optional overlap, walking the filter-applied Markdown sequentially. Use
this when headings are absent or uneven, or when you need a strict
size budget for embedding input.

```ruby
# Char-budgeted windows with overlap
Inkmark.chunks_by_size(doc, chars: 500, overlap: 50)

# Word budget, word-boundary cuts
Inkmark.chunks_by_size(doc, words: 120, overlap: 15, at: :word)

# Dual budget: cut at whichever is reached first
Inkmark.chunks_by_size(doc, chars: 1000, words: 200)
```

Each window:

```ruby
{
  index:   0,         # 0-based sequence position
  content: "..."      # filter-applied Markdown slice
  # character_count, word_count added when options: { statistics: true }
}
```

**Boundary modes.** `at: :block` (default) cuts only between top-level
Markdown blocks—output stays valid Markdown, and a single block that
exceeds the budget is emitted as its own window rather than silently
dropped. `at: :word` serializes the full filtered Markdown and cuts at
the last Unicode word boundary that fits—tighter fit but may split
open constructs.

**Overlap.** Measured in chars. Each new window begins with the
trailing `overlap` chars of the previous window, so adjacent chunks
share context—useful when an embedding model's attention benefits
from neighbor overlap. Must be less than `chars:` when both are set.

**Validation.** `chars` or `words` required (at least one). Both must
be positive. `overlap` defaults to 0, must be non-negative, and must be
less than `chars` when `chars` is set. Invalid combinations raise
`ArgumentError` at the Ruby boundary—silent clamping would mask
bugs like swapped args.

#### Heading vs size: which to use

`chunks_by_heading` for docs where headings encode meaningful
structure (articles, specs, READMEs). Each chunk carries heading,
level, id, and breadcrumb metadata—retrieval benefits from that
context.

`chunks_by_size` for unstructured or uneven-heading docs, or when a
hard size ceiling matters more than document structure. No structural
metadata; windows are just positioned slices.

You can compose them for a hybrid "heading-based, but size-capped"
pattern:

```ruby
Inkmark.chunks_by_heading(doc).flat_map do |c|
  if c[:content].size > 2000
    Inkmark.chunks_by_size(c[:content], chars: 500, overlap: 50)
  else
    [c]
  end
end
```

## Truncation

`Inkmark.truncate_markdown` caps a document at a character and/or word
budget, cutting at either a Markdown block boundary (valid structure) or
a Unicode word boundary (tighter fit, may split an open construct).
Designed for LLM context-window budgeting and RAG chunk normalization.

```ruby
# Block-boundary cut: last complete block that fits, output is valid Markdown
Inkmark.truncate_markdown(doc, chars: 4000, at: :block)

# Word-boundary cut: last word that fits, output may split open constructs
Inkmark.truncate_markdown(doc, chars: 4000, at: :word)

# Dual budget: cut at whichever limit is hit first
Inkmark.truncate_markdown(doc, chars: 4000, words: 500, at: :word)

# Suppress the marker
Inkmark.truncate_markdown(doc, chars: 4000, at: :block, marker: nil)

# Custom marker
Inkmark.truncate_markdown(doc, chars: 4000, at: :block, marker: "[…]")
```

Default marker is `"…"`. When appended, it counts toward the
budget—`chars: 4000` always yields output ≤ 4000 codepoints.

**Behavior:**

- **Source fits the budget**: returned unchanged (no marker).
- **First block alone exceeds the budget** (block mode): empty string.
  Honest to "no block fits"; fall through to word-mode truncation if you
  want a best-effort cut.
- **Marker too large for the budget**: raises `ArgumentError`.
- **Filter pipeline**: `emoji_shortcodes`, `links: { autolink: true }`, host/scheme
  allowlists etc. run before truncation, so the measured output matches
  what downstream tools consume.

### Per-section truncation

`chunks_by_heading` accepts a `truncate:` kwarg that applies the same
contract to every section's `:content` independently:

```ruby
Inkmark.chunks_by_heading(doc, truncate: {chars: 500, at: :block})
```

Each section's content is cut to the 500-char budget; metadata
(`:heading`, `:level`, `:id`, `:breadcrumb`) stays intact. When
`statistics: true` is also set, `:character_count` / `:word_count` are
recomputed against the truncated content.

```ruby
Inkmark.chunks_by_heading(doc,
  options: {statistics: true},
  truncate: {chars: 500, at: :block, marker: "…"}
)
# => each entry: { heading:, level:, id:, breadcrumb:,
#                  character_count:, word_count:, content: (≤ 500 chars) }
```

Because sections are hierarchical (a parent section's `:content`
includes nested subsections), applying the same budget to every entry
means each chunk stands alone as a self-contained, budget-capped unit.

## Plain-text extraction

`Inkmark#to_plain_text` strips all Markdown syntax and returns inline content as
plain text. Designed for embedding models, token counting, LLM input, and any
downstream consumer that treats Markdown formatting as noise.

```ruby
Inkmark.to_plain_text("**bold** and [a link](https://example.com)")
# => "bold and a link (https://example.com)\n"

g = Inkmark.new(source, options: { emoji_shortcodes: true, links: { autolink: true } })
g.to_plain_text
```

The same event-level filters (emoji replacement, autolink, host/scheme
allowlists, etc.) run before plain-text serialization, so preprocessing passes
apply consistently across `to_html`, `to_markdown`, and `to_plain_text`.

### Output grammar

| Element | Plain-text form |
|---|---|
| `**bold**`, `*italic*`, `~~strike~~` | inner text only |
| `` `code` `` | inner text (no backticks) |
| `[text](url)` | `text (url)` |
| `<https://x.com>` (autolink) | `https://x.com` (collapses when text == url) |
| `![alt](src)` | `alt (src)` |
| `# Heading` | plain text with blank line above/below |
| `> quote` | every line prefixed with `> ` (email-style; nests) |
| `- item` / `1. item` | `- ` / `1. ` bullets; 2-space indent per nesting |
| `- [x] task` | `- task` (checkbox dropped) |
| tables | header row `\t`-joined, blank line, body rows `\t`-joined |
| ``` ```code``` ``` | raw content, blank line above/below |
| `---` | `---` surrounded by blank lines |
| `[^foo]` | `[foo]` |
| `[^foo]: body` | appended at document end as `[foo]: body` |
| soft break | space |
| hard break | `\n` |
| raw HTML | stripped by default; passes through when `raw_html: true` |

Blank lines inside a blockquote emit a bare `>` marker (matching email quoting
conventions; no trailing whitespace).

## Markdown-to-Markdown pipeline

`Inkmark#to_markdown` runs the same event-level filter pipeline as `to_html` and
serializes the result back to Markdown text. Use it as a preprocessing step in
pipelines that consume Markdown: LLM prompts, secondary renderers, content
storage, or any stage that needs clean Markdown rather than HTML.

```ruby
# Class-method shortcut
Inkmark.to_markdown("**bold** :rocket:", options: { emoji_shortcodes: true })
# => "**bold** 🚀"

# Instance form—the same options object drives both outputs
g = Inkmark.new(source, options: {
  emoji_shortcodes: true,
  links: { allowed_hosts: ["trusted.com", "*.trusted.com"] }
})
g.to_markdown   # filtered Markdown for pipeline
g.to_html       # rendered HTML for display
```

### Choosing filters for a Markdown pipeline

Inkmark's filters fall into two groups depending on what they emit:

**Markdown-native filters** transform the event stream without producing HTML.
Their output is standard Markdown and is safe to pass to any downstream
consumer:

| Filter | Effect in `to_markdown` |
|---|---|
| `emoji_shortcodes` | `:rocket:` → `🚀` in the output text |
| `links: { autolink: true }` | bare `https://x.com` → `[https://x.com](https://x.com)` |
| `links: { allowed_hosts:, allowed_schemes: }` | disallowed links unwrapped to plain text |
| `images: { allowed_hosts:, allowed_schemes: }` | disallowed images dropped to alt text |
| `smart_punctuation` | `"..."` → `"…"` etc. (text-only transformation) |

**HTML-emitting filters** synthesize raw `<...>` markup. When these are active
and you call `to_markdown`, that markup is embedded verbatim in the output. Raw
HTML blocks are valid CommonMark, but they may break or confuse downstream
consumers—especially LLMs and renderers that do not expect HTML inside
Markdown:

| Filter | What ends up in the Markdown |
|---|---|
| `syntax_highlight` | fenced code blocks become `<pre><code><span class=...>` HTML |
| `images: { lazy: true }` | images become `<img loading="lazy" decoding="async" ...>` HTML |
| `links: { nofollow: true }` | links become `<a rel="nofollow noopener" ...>` HTML |

**Recommendation:** disable HTML-emitting filters when calling `to_markdown`.
They are designed for final HTML output and produce hard-to-process markup in a
Markdown pipeline:

```ruby
Inkmark.to_markdown(source, options: {
  # Markdown-native—safe to enable
  emoji_shortcodes: true,
  links:  { allowed_schemes: %w[http https mailto], nofollow: false },
  images: { lazy: false },

  # HTML-emitting—turn off for clean Markdown output
  syntax_highlight: false,         # would embed <pre><span...> blocks
})
```

## Event handlers

Register handlers with `#on` to inspect or transform document elements as they
are parsed. Handlers fire **post-order**—children before parents—so when a
`:table` handler runs, its rows and cells are already available. Returns `self`
for chaining.

```ruby
md = Inkmark.new(source)

md.on(:heading) { |h| ... }
  .on(:image)   { |img| ... }
  .on(:link)    { |l| ... }
```

Two entry points trigger handlers:

- **`#walk`**—fires handlers without producing HTML. Use it for analysis:
  collecting specific elements, validating content, extracting structured data.
  For built-in heading/link/image/word-count collection, see `statistics: true`.
- **`#to_html`**—fires handlers then renders. Mutations made inside a handler
  change what ends up in the HTML.

### Collecting data with `#walk`

```ruby
# Check that every image has alt text
md = Inkmark.new(source)
missing_alt = []
md.on(:image) { |img| missing_alt << img.dest if img.text.empty? }
md.walk
raise "Images missing alt text: #{missing_alt.join(', ')}" if missing_alt.any?
```

```ruby
# Collect every fenced code block language used in the document
languages = Set.new
md.on(:code_block) { |c| languages << c.lang if c.lang && !c.lang.empty? }
md.walk
```

```ruby
# Validate that no link points to a deprecated domain
deprecated = /old-docs\.example\.com/
md.on(:link) { |l| warn "Deprecated link: #{l.dest}" if l.dest =~ deprecated }
md.walk
```

### Rewriting output with `#to_html`

#### Image CDN rewriting

Set `dest=` to redirect images to a CDN. The change is reflected in the
rendered `<img src>`:

```ruby
md = Inkmark.new(source)
md.on(:image) do |img|
  img.dest = "https://cdn.example.net/#{File.basename(img.dest)}"
end
html = md.to_html
```

#### Rewriting link destinations

```ruby
md.on(:link) do |l|
  if l.dest.start_with?("http")
    l.html = %(<a href="#{l.dest}" target="_blank" rel="noopener">#{l.text}</a>)
  end
end
```

#### Shifting heading levels

Bump every heading down one level so the document fits inside a layout that
reserves `<h1>` for the page title:

```ruby
md = Inkmark.new(source)
md.on(:heading) { |h| h.level = [h.level + 1, 6].min }
html = md.to_html
```

#### Custom code block rendering

Intercept fenced code blocks by language tag. Setting `html=` skips Inkmark's
default `<pre><code>` output—and the `syntax_highlight` filter, even if
enabled:

```ruby
md = Inkmark.new(source)
md.on(:code_block) do |c|
  case c.lang
  when "mermaid"
    c.html = %(<div class="mermaid">#{c.text}</div>\n)
  when "math"
    c.html = %(<div class="math">\\[#{c.text}\\]</div>\n)
  end
end
html = md.to_html
```

#### Custom directives in paragraphs

Match a special directive syntax and replace the paragraph with a component:

```ruby
# Markdown:
#   @available_since rails=7.1 ruby=3.2
#
md.on(:paragraph) do |p|
  next unless p.text =~ /\A@available_since\s+(.+)\z/
  attrs = $1.scan(/(\w+)=(\S+)/).map { |k, v| %( #{k}="#{v}") }.join
  p.html = %(<AvailableSince#{attrs} />\n)
end
```

#### Replacing with Markdown

Use `markdown=` when the replacement is itself Markdown rather than raw HTML.
The replacement is parsed with the same options as the main document—emoji
expansion, heading IDs, raw HTML suppression—and is subject to the same
post-render filters (`syntax_highlight`, allowlists, `images: { lazy: true }`, `links: { nofollow: true }`).
Handlers do **not** fire on elements within the replacement.
`html=` takes priority when both are set on the same event.

```ruby
md = Inkmark.new(source)
md.on(:paragraph) do |p|
  if p.text.start_with?("@note ")
    body = p.text.sub(/\A@note /, "")
    p.markdown = "> **Note:** #{body}"
  end
end
html = md.to_html
```

#### Suppressing elements

Call `delete` on any event to omit it from the output. Children are suppressed
along with their parent:

```ruby
md.on(:image)   { |img| img.delete }                                  # all images
md.on(:heading) { |h|   h.delete if h.text.start_with?("INTERNAL:") } # by content
```

#### Inline code annotation

`:code` fires for inline backtick spans. Use it to add links or decoration:

```ruby
md.on(:code) do |c|
  if c.text =~ /\A[A-Z][A-Za-z]+#\w+\z/    # e.g. String#split
    c.html = %(<a href="/api/#{c.text.tr('#', '/')}"><code>#{c.text}</code></a>)
  end
end
```

### Children and tree context

Container elements expose their child events (lazy, cached):

```ruby
md.on(:table) do |t|
  rows = t.children_of(:table_row)
  rows.each_with_index do |row, i|
    cells = row.children_of(:table_cell).map(&:text)
    puts "Row #{i}: #{cells.join(' | ')}"
  end
end
```

Use `parent_kind` and `ancestor_kinds` for context-sensitive decisions:

```ruby
# Skip decorative images that are already inside a link
md.on(:image) { |img| img.delete if img.ancestor_kinds.include?(:link) }

# Only process top-level paragraphs
md.on(:paragraph) { |p| next unless p.parent_kind.nil? }
```

`depth` gives the nesting level (0 = top-level block):

```ruby
md.on(:blockquote) { |b| puts "blockquote at depth #{b.depth}" }
md.on(:paragraph)  { |p| puts "paragraph at depth #{p.depth}" }
# A paragraph inside a blockquote has depth 1.
```

### Source byte ranges

`byte_range` is an exclusive Ruby Range (`start...end`) that lets you slice the
original source to recover the raw Markdown for any element:

```ruby
source = File.read("post.md")
md = Inkmark.new(source)
md.on(:heading) do |h|
  puts "#{h.byte_range}: #{source[h.byte_range].inspect}"
end
md.walk
```

Populated for all container kinds and the leaf kinds `:code`, `:rule`,
`:inline_math`, `:display_math`. Returns `nil` for `:text`, `:soft_break`,
and `:hard_break`. Also `nil` for `:link` when `links: { autolink: true }` is enabled
(the autolink filter inserts new link events that would shift the offset queue).

### Event object reference

Every handler receives a `Inkmark::Event` with these fields and methods:

| Field / method | Type | Description |
|---|---|---|
| `kind` | `Symbol` | Element kind, e.g. `:heading`, `:image` |
| `text` | `String` | Plain text of all descendant text nodes |
| `depth` | `Integer` | Nesting depth; 0 = top-level block |
| `parent_kind` | `Symbol, nil` | Kind of the immediate parent, or `nil` at root |
| `ancestor_kinds` | `Array<Symbol>` | Ancestor kinds, nearest first |
| `byte_range` | `Range, nil` | Byte offsets in the original source string |
| `children` | `Array<Event>` | Direct child events (containers only) |
| `children_of(kind)` | `Array<Event>` | Children filtered by kind |
| `delete` |—| Suppress this element from output |
| `deleted?` | `Boolean` | True if `delete` was called |
| `html=` | `String, nil` | Replace output with a raw HTML string |
| `markdown=` | `String, nil` | Replace output by re-rendering a Markdown string |
| `dest=` | `String, nil` | Rewrite URL on `:link` / `:image` |
| `title=` | `String, nil` | Rewrite title attribute on `:link` / `:image` |
| `level=` | `Integer, nil` | Change heading level (1–6) on `:heading` |
| `id=` | `String, nil` | Change `id` attribute on `:heading` |

#### Per-kind field availability

**Container kinds**: handler fires after all children are processed:

| Kind | Readable | Mutable |
|---|---|---|
| `:heading` | `text`, `level`, `id` | `level=`, `id=`, `html=`, `markdown=` |
| `:paragraph` | `text` | `html=`, `markdown=` |
| `:blockquote` | `text` | `html=`, `markdown=` |
| `:list` |—| `html=`, `markdown=` |
| `:ordered_list` |—| `html=`, `markdown=` |
| `:list_item` | `text` | `html=`, `markdown=` |
| `:code_block` | `text`, `lang` | `html=`, `markdown=` |
| `:table` |—| `html=`, `markdown=` |
| `:table_head` |—| `html=`, `markdown=` |
| `:table_row` | `text` | `html=`, `markdown=` |
| `:table_cell` | `text` | `html=`, `markdown=` |
| `:emphasis` | `text` | `html=`, `markdown=` |
| `:strong` | `text` | `html=`, `markdown=` |
| `:strikethrough` | `text` | `html=`, `markdown=` |
| `:link` | `text`, `dest`, `title` | `dest=`, `title=`, `html=`, `markdown=` |
| `:image` | `text` (alt), `dest`, `title` | `dest=`, `title=`, `html=`, `markdown=` |
| `:footnote_definition` | `text` | `html=`, `markdown=` |

**Leaf kinds**: no children; handler fires on the event itself:

| Kind | Readable | Mutable |
|---|---|---|
| `:code` | `text` | `html=` |
| `:text` | `text` | `html=` |
| `:html` | `text` | `html=` |
| `:rule` |—| `html=` |
| `:soft_break` |—| `html=` |
| `:hard_break` |—| `html=` |
| `:footnote_reference` | `text` | `html=` |

All kinds expose `depth`, `parent_kind`, `ancestor_kinds`, `byte_range`,
`children`, `children_of`, `delete`, `deleted?`.

`:code_block` `text` and `source` are identical—`source` is an alias for
readability when treating the field as raw source code.

### Filter interaction

Enrichment filters run **before** handlers. Handlers always see:

- Emoji already resolved (`emoji_shortcodes: true`)—`h.text` contains `"🚀"`,
  not `":rocket:"`
- Bare URLs already autolinked (`links: { autolink: true }`)—they appear as `:link` events
- Heading `id` already set (`headings: { ids: true }`)—`h.id` is populated

Post-render filters (`syntax_highlight`, allowlists, `images: { lazy: true }`,
`links: { nofollow: true }`) run **after** handlers:

- `:code_block` events are still `:code_block`, not opaque HTML, even when
  `syntax_highlight: true`—setting `html=` on a code block overrides the
  highlighter
- Handler-set `dest=` values pass through host and scheme allowlists

## Benchmarks

Inkmark ships a benchmark harness comparing it against `kramdown`,
`commonmarker`, `redcarpet`, `markly`, and `rdiscount` on a sweep of real
markdown inputs. 

Measuring apples to apples: every adapter is tuned for **feature parity** with
Inkmark's defaults—CommonMark + core GFM (tables, strikethrough, tasklists,
footnotes, tagfilter), no typographics, no autolink, no syntax highlighting,
no heading-id slugging.

Run locally:

```bash
bundle config set with benchmark
bundle install
bundle exec rake benchmark
```

### Assets

| Asset | Size | What it exercises |
|---|---:|---|
| `commonmark-spec` | 201.3 KB | CommonMark spec—code-block-heavy, edge-case-heavy |
| `commonmarker-readme` | 17.0 KB | Real-world commonmarker README—options tables, fenced code |
| `redcarpet-readme` | 14.0 KB | Real-world redcarpet README—prose + code samples |
| `redcarpet-benchmark` | 8.0 KB | Classic redcarpet bench corpus—heavy emphasis / inline parsing |
| `large-4k` | 3.7 KB | dotenv README—mixed prose, code blocks, tables |
| `medium-1k` | 1.0 KB | Faraday README header—images, badges, inline links |
| `small-512b` | 0.5 KB | Short README section with headings and bullet lists |
| `tiny-256b` | 0.3 KB | 3-line CommonMark snippet—parser setup/overhead-bound |

See `benchmarks/NOTICE` for attribution on the vendored test inputs.

### Results

Numbers below are from AWS EC2 `c7a.large` (AMD EPYC), Ruby 4.0.2 with YJIT on.
Each engine uses its idiomatic "hot path"—Inkmark relies on its cached default
options, Redcarpet reuses one pre-built `Markdown` object. Iterations per
second, higher is better.

**`commonmark-spec` (201.3 KB)**
```
inkmark:        1,172 i/s
redcarpet:        908 i/s -  1.29x slower
markly:           453 i/s -  2.59x slower
commonmarker:     345 i/s -  3.40x slower
rdiscount:        212 i/s -  5.53x slower
kramdown:          26 i/s - 45.08x slower
```

**`commonmarker-readme` (16.9 KB)**
```
inkmark:       16,658 i/s
redcarpet:     12,988 i/s -   1.28x slower
commonmarker:   4,268 i/s -   3.90x slower
markly:         3,974 i/s -   4.19x slower
rdiscount:      2,676 i/s -   6.22x slower
kramdown:         113 i/s - 147.42x slower
```

**`redcarpet-readme` (14.0 KB)**
```
inkmark:       17,343 i/s
redcarpet:     13,587 i/s -  1.28x slower
markly:         5,455 i/s -  3.18x slower
commonmarker:   4,890 i/s -  3.55x slower
rdiscount:      3,336 i/s -  5.20x slower
kramdown:         208 i/s - 83.38x slower
```

**`redcarpet-benchmark` (8.0 KB)**
```
inkmark:       27,634 i/s
redcarpet:     23,777 i/s -  1.16x slower
markly:         9,346 i/s -  2.96x slower
commonmarker:   7,805 i/s -  3.54x slower
rdiscount:      6,201 i/s -  4.46x slower
kramdown:         367 i/s - 75.30x slower
```

**`large-4k` (3.7 KB)**
```
inkmark:       64,051 i/s
redcarpet:     58,420 i/s -   1.10x slower
markly:        22,500 i/s -   2.85x slower
commonmarker:  18,053 i/s -   3.55x slower
rdiscount:     13,839 i/s -   4.63x slower
kramdown:         624 i/s - 102.64x slower
```

**`medium-1k` (1.0 KB)**
```
redcarpet:    216,968 i/s
inkmark:      213,478 i/s -  1.02x slower
markly:        70,251 i/s -  3.09x slower
commonmarker:  46,357 i/s -  4.68x slower
rdiscount:     45,880 i/s -  4.73x slower
kramdown:       2,813 i/s - 77.13x slower
```

**`small-512b` (0.5 KB)**
```
inkmark:      388,266 i/s
redcarpet:    368,401 i/s -  1.05x slower
rdiscount:     74,032 i/s -  5.24x slower
markly:        61,175 i/s -  6.35x slower
commonmarker:  46,658 i/s -  8.32x slower
kramdown:       3,952 i/s - 98.25x slower
```

**`tiny-256b` (0.3 KB)**
```
redcarpet:    535,972 i/s
inkmark:      511,019 i/s -   1.05x slower
rdiscount:     99,001 i/s -   5.41x slower
markly:        96,159 i/s -   5.57x slower
commonmarker:  57,704 i/s -   9.29x slower
kramdown:       4,117 i/s - 130.18x slower
```

## Contributing

Bug reports and pull requests are welcome on GitHub at
https://github.com/yaroslav/inkmark.

## Acknowledgements

Inkmark is built with:

[pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) by Raph Levien, Marcus Klaas de Vries, Martín Pozo, Michael Howell, Roope Salmi and Martin Geisler;

[Magnus](https://github.com/matsadler/magnus) by Matthew Sadler;

[syntect](https://github.com/trishume/syntect) by Tristan Hume, Keith Hall, Google Inc and other contributors;

And other Rust crates—thanks to their authors.

Thanks to Julik Tarkhanov for short but useful brainstorming sessions.

## License

The gem is available as open source under the terms of the
[MIT License](LICENSE.txt). Third-party content (benchmark assets, CommonMark
spec) is attributed in `NOTICE` and `benchmarks/NOTICE`.
