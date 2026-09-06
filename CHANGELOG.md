## [Unreleased]

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
