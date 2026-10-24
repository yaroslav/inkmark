## [0.3.1] - 2026-10-24

- Re-release of 0.3.0, which was yanked.
- Breaking: Markdown output (`to_markdown`, `chunks_by_heading`, `chunks_by_size`, `truncate_markdown`) uses pulldown-cmark-to-cmark 23; escaping and formatting differ from 0.2.
- Fix raw HTML leaking into Markdown output with `raw_html: false` when a line held more than one tag.
- Fix `chunks_by_size` and `truncate_markdown` (`at: :block`) joining blocks without a blank line, which merged adjacent paragraphs.
- Fix Markdown output crashing on an image nested in another image's alt text with `images: { lazy: true }`, `allowed_hosts`, or `allowed_schemes`.
- `chunks_by_heading` starts sections only at top-level headings; a heading inside a block quote or list item no longer splits it.
- Raise `Inkmark::Error` on Markdown serialization failures instead of crashing the process.

## [0.3.0] - 2026-10-24 [YANKED]

- Yanked: a RubyGems issue interrupted publishing, so only one platform gem was released. Its changes shipped in 0.3.1.

## [0.2.2] - 2026-10-24

- Updated dependencies, including pulldown-cmark 0.13.5 (upstream security fixes) and magnus 0.9.3.

## [0.2.1] - 2026-09-25

- Updated dependencies, including magnus 0.9.

## [0.2.0] - 2026-09-06

- Ractor-safe: the native extension declares `rb_ext_ractor_safe`, and every public API works from non-main Ractors.
- `Inkmark.default_options` is now frozen; configure process-wide defaults with `Inkmark.configure { |o| ... }` or `Inkmark.default_options=`.
- Load YAML on the first `frontmatter` parse instead of at require time.
- Updated dependencies.

## [0.1.4] - 2026-06-25

- Fix `frontmatter: true` leaking the frontmatter block into `to_markdown`, `chunks_by_heading`, and `chunks_by_size` output. Bug report by @freesteph [#3]

## [0.1.3] - 2026-06-21

- Fix possible XSS via unescaped language tag in syntax-highlighted code blocks.

## [0.1.2] - 2026-06-21

- Fix `Inkmark.truncate_markdown` raising `TypeError` when called without explicit `options:`.
- Update dependencies on the Rust side.

## [0.1.1] - 2026-04-22

- Strip DWARF debug info from shipped Linux and Windows binaries via `strip = "debuginfo"`.

## [0.1.0] - 2026-04-22

- Initial public release
