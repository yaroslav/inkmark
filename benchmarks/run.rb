# frozen_string_literal: true

require "bundler/setup"
require "benchmark/ips"

RubyVM::YJIT.enable if defined?(RubyVM::YJIT)

require "inkmark"

GEMS = %w[inkmark commonmarker markly kramdown redcarpet rdiscount]
LOADED = {}
GEMS.each do |gem_name|
  require gem_name
  LOADED[gem_name] = true
rescue LoadError => e
  warn "skipping #{gem_name}: #{e.message}"
end

# Pre-built Redcarpet parser, reused across every call by the
# "redcarpet" adapter. The "redcarpet (per-call parser)" variant
# builds a fresh one inside the lambda.
REDCARPET_PARSER = LOADED["redcarpet"] &&
  Redcarpet::Markdown.new(
    Redcarpet::Render::HTML.new,
    tables: true, strikethrough: true,
    fenced_code_blocks: true, footnotes: true
  )

# [label, gem_name, adapter_proc]. Every adapter is tuned for parity
# with Inkmark's default feature set: CommonMark + the five core GFM
# extensions (tables, strikethrough, tasklists, footnotes, tagfilter).
# No typographics, no autolink, no syntax highlighting, no heading-id
# slugging, no emoji shortcodes. Engines whose default configuration
# does more than that get the extras turned off.
#
# Two engines (inkmark, redcarpet) appear twice: once with options/
# parser reused across calls (the hot path) and once with per-call
# construction (the cost a one-shot `options: {...}` caller actually
# pays). The other engines take their options inline on every call
# by design; one row per engine covers them.
ADAPTERS = [
  # Inkmark, hot path: default options.
  ["inkmark", "inkmark",
    ->(md) { Inkmark.to_html(md) }],

  # Inkmark, per-call options: `{tables: true}` is the
  # minimum that forces `native_hash_from` to walk its override loop
  #
  # `tables: true` matches the :gfm default so the rendered
  # HTML is byte-identical to the hot-path adapter above — keeping
  # this row directly comparable to `redcarpet (per-call parser)`,
  # which also constructs fresh options per call for the same GFM
  # feature set.
  ["inkmark (per-call opts)", "inkmark",
    ->(md) { Inkmark.to_html(md, options: {tables: true}) }],

  # Commonmarker
  ["commonmarker", "commonmarker",
    ->(md) {
      Commonmarker.to_html(
        md,
        options: {
          render: {hardbreaks: false},
          extension: {
            table: true, strikethrough: true, tasklist: true,
            footnotes: true, tagfilter: true,
            autolink: false, shortcodes: false
          }
        },
        plugins: {}
      )
    }],

  # Markly (cmark-gfm). Add :tagfilter for GFM parity and the
  # FOOTNOTES parse flag to match Inkmark's footnote handling.
  ["markly", "markly",
    ->(md) {
      Markly.render_html(
        md,
        flags: Markly::FOOTNOTES,
        extensions: [:table, :strikethrough, :tasklist, :tagfilter]
      )
    }],

  # Kramdown: disable auto_ids and syntax_highlighter for parity.
  # smart_quotes is always-on in kramdown with no clean off switch.
  ["kramdown", "kramdown",
    ->(md) {
      Kramdown::Document.new(
        md,
        auto_ids: false,
        syntax_highlighter: nil
      ).to_html
    }],

  # Redcarpet, hot path: reuse one pre-built Markdown object.
  ["redcarpet", "redcarpet",
    ->(md) { REDCARPET_PARSER.render(md) }],

  # Redcarpet, per-call parser: build a fresh Redcarpet::Markdown
  # every call.
  ["redcarpet (per-call parser)", "redcarpet",
    ->(md) {
      Redcarpet::Markdown.new(
        Redcarpet::Render::HTML.new,
        tables: true, strikethrough: true,
        fenced_code_blocks: true, footnotes: true
      ).render(md)
    }],

  # RDiscount: tables / strikethrough / superscript on by default.
  # Pass only :footnotes; drop :smart (typographics) and :autolink
  # for parity.
  ["rdiscount", "rdiscount",
    ->(md) {
      RDiscount.new(md, :footnotes).to_html
    }]
]

ENGINES = ADAPTERS.each_with_object({}) do |(label, gem_name, adapter), h|
  h[label] = adapter if LOADED[gem_name]
end

abort "no benchmark assets found" if Dir["#{__dir__}/assets/*.md"].empty?
abort "no engines available" if ENGINES.empty?

ASSETS = Dir["#{__dir__}/assets/*.md"].sort.to_h do |path|
  [File.basename(path, ".md"), File.read(path)]
end

ASSETS.each do |name, source|
  size_kb = (source.bytesize / 1024.0).round(1)
  puts "\n=== #{name} (#{size_kb} KB) ==="

  Benchmark.ips do |x|
    x.config(time: 5, warmup: 2)
    ENGINES.each { |label, fn| x.report(label) { fn.call(source) } }
    x.compare!
  end
end
