# frozen_string_literal: true

require "spec_helper"
require "inkmark"

RSpec.describe Inkmark do
  describe ".new" do
    context "with a markdown string" do
      it "stores the source unchanged" do
        g = described_class.new("# hi")
        expect(g.source).to eq("# hi")
      end
    end

    context "with nil" do
      it "normalizes source to an empty string" do
        expect(described_class.new(nil).source).to eq("")
      end
    end

    context "with a non-String input" do
      it "coerces via #to_s" do
        expect(described_class.new(42).source).to eq("42")
      end
    end
  end

  describe "#to_s" do
    it "returns the stored source unchanged" do
      expect(described_class.new("# Hello").to_s).to eq("# Hello")
    end

    it "is empty for a nil source" do
      expect(described_class.new(nil).to_s).to eq("")
    end

    it "does not run the filter pipeline" do
      g = described_class.new(":rocket:", options: {emoji_shortcodes: true})
      expect(g.to_s).to eq(":rocket:")
    end
  end

  describe "#source=" do
    it "normalizes nil to an empty string" do
      g = described_class.new("hi")
      g.source = nil
      expect(g.source).to eq("")
    end

    it "coerces non-String via #to_s" do
      g = described_class.new("hi")
      g.source = 42
      expect(g.source).to eq("42")
    end
  end

  describe "#options" do
    context "when no options were passed" do
      it "returns an Options instance with defaults" do
        expect(described_class.new("x").options).to eq(Inkmark::Options.new)
      end
    end

    context "when a hash is passed" do
      it "merges the hash on top of defaults" do
        g = described_class.new("x", options: {tables: false})
        expect(g.options.tables).to be false
        expect(g.options.gfm).to be true
      end
    end

    context "when a Inkmark::Options is passed" do
      it "dups it so subsequent mutations don't bleed" do
        o = Inkmark::Options.new(tables: false)
        g = described_class.new("x", options: o)
        o.tables = true
        expect(g.options.tables).to be false
      end
    end
  end

  describe "#to_html" do
    context "with a markdown string" do
      it "returns the rendered HTML" do
        expect(described_class.new("**hi**").to_html).to eq("<p><strong>hi</strong></p>\n")
      end
    end

    context "with an empty source" do
      it "returns an empty string without calling the native extension" do
        expect(described_class.new("").to_html).to eq("")
      end
    end

    context "with nil source" do
      it "returns an empty string without calling the native extension" do
        expect(described_class.new(nil).to_html).to eq("")
      end
    end

    context "when options change between calls" do
      it "uses the current options at each render" do
        g = described_class.new("|a|b|\n|-|-|\n|1|2|", options: {tables: false})
        expect(g.to_html).not_to include("<table>")
        g.options.tables = true
        expect(g.to_html).to include("<table>")
      end
    end
  end

  describe ".to_html" do
    context "with a markdown string" do
      it "returns the rendered HTML" do
        expect(described_class.to_html("**hi**")).to eq("<p><strong>hi</strong></p>\n")
      end
    end

    context "with nil" do
      it "returns an empty string" do
        expect(described_class.to_html(nil)).to eq("")
      end
    end

    context "with an inline options hash" do
      it "applies the options to the render" do
        expect(described_class.to_html("~~x~~", options: {strikethrough: true}))
          .to eq("<p><del>x</del></p>\n")
      end
    end
  end

  describe ".default_options" do
    before { described_class.default_options = Inkmark::Options.new }
    after { described_class.default_options = Inkmark::Options.new }

    it "returns a Inkmark::Options instance" do
      expect(described_class.default_options).to be_a(Inkmark::Options)
    end

    it "seeds new instances when no options are passed" do
      described_class.default_options = Inkmark::Options.new(tables: false)
      g = described_class.new("hi")
      expect(g.options.tables).to be false
    end

    it "does not let defaults bleed from mutating one instance" do
      described_class.default_options = Inkmark::Options.new
      g = described_class.new("hi")
      g.options.tables = false
      expect(described_class.default_options.tables).to be true
    end
  end

  describe "heading IDs" do
    context "with headings: { ids: true }" do
      it "generates a lowercased dash-separated id from heading text" do
        html = described_class.to_html("# Hello, World!", options: {headings: {ids: true}})
        expect(html).to include('<h1 id="hello-world">Hello, World!</h1>')
      end

      it "slugs a plain word unchanged except for case" do
        html = described_class.to_html("# Introduction", options: {headings: {ids: true}})
        expect(html).to include('<h1 id="introduction">')
      end

      it "suffixes duplicate slugs with a counter" do
        html = described_class.to_html(
          "# Intro\n\n## Intro\n\n### Intro\n",
          options: {headings: {ids: true}}
        )
        expect(html).to include('<h1 id="intro">')
        expect(html).to include('<h2 id="intro-1">')
        expect(html).to include('<h3 id="intro-2">')
      end

      it "preserves user-supplied ids from headings: { attributes: true }" do
        html = described_class.to_html(
          "# Foo {#custom}",
          options: {headings: {ids: true, attributes: true}}
        )
        expect(html).to include('id="custom"')
        expect(html).not_to include('id="foo"')
      end

      it "transliterates Latin diacritics" do
        html = described_class.to_html("# Résumé", options: {headings: {ids: true}})
        expect(html).to include('id="resume"')
      end

      it "transliterates Cyrillic via deunicode's default table" do
        html = described_class.to_html("# Лев Толстой", options: {headings: {ids: true}})
        expect(html).to include('id="lev-tolstoi"')
      end
    end

    context "with headings: { ids: false } (default)" do
      it "does not add ids" do
        expect(described_class.to_html("# Hello")).to eq("<h1>Hello</h1>\n")
      end
    end
  end

  describe "frontmatter" do
    let(:source) do
      "---\ntitle: Hello World\nauthor: Jane Doe\ntags:\n  - ruby\n  - markdown\n---\n\n# Content"
    end

    context "with frontmatter: true" do
      it "returns a parsed Hash" do
        md = described_class.new(source, options: {frontmatter: true})
        fm = md.frontmatter
        expect(fm).to eq({"title" => "Hello World", "author" => "Jane Doe", "tags" => ["ruby", "markdown"]})
      end

      it "strips frontmatter from rendered HTML" do
        md = described_class.new(source, options: {frontmatter: true})
        expect(md.to_html).not_to include("title:")
        expect(md.to_html).to include("<h1>Content</h1>")
      end

      it "returns nil when no frontmatter block exists" do
        md = described_class.new("# Just content", options: {frontmatter: true})
        expect(md.frontmatter).to be_nil
      end

      it "handles numeric and boolean values" do
        src = "---\ncount: 42\ndraft: true\n---\n\nBody"
        md = described_class.new(src, options: {frontmatter: true})
        expect(md.frontmatter["count"]).to eq(42)
        expect(md.frontmatter["draft"]).to be true
      end
    end

    context "with frontmatter: false (default)" do
      it "returns nil" do
        expect(described_class.new(source).frontmatter).to be_nil
      end
    end
  end

  describe "table of contents" do
    let(:source) { "# Hello\n\n## World\n\n### Deep\n" }

    context "with toc: true" do
      it "returns a Inkmark::Toc value object" do
        md = described_class.new(source, options: {toc: true})
        expect(md.toc).to be_a(Inkmark::Toc)
      end

      it "renders to markdown via #to_markdown" do
        md = described_class.new(source, options: {toc: true})
        toc_md = md.toc.to_markdown
        expect(toc_md).to include("[Hello](#hello)")
        expect(toc_md).to include("[World](#world)")
        expect(toc_md).to include("[Deep](#deep)")
      end

      it "renders to HTML via #to_html" do
        md = described_class.new(source, options: {toc: true})
        html = md.toc.to_html
        expect(html).to include('<a href="#hello">Hello</a>')
        expect(html).to include('<a href="#world">World</a>')
      end

      it "coerces to the markdown form via #to_s" do
        md = described_class.new(source, options: {toc: true})
        expect(md.toc.to_s).to eq(md.toc.to_markdown)
      end

      it "generates heading IDs in the rendered HTML even without headings: { ids: true }" do
        md = described_class.new(source, options: {toc: true})
        html = md.to_html
        expect(html).to include('id="hello"')
        expect(html).to include('id="world"')
      end

      it "uses the same slugs in TOC anchors and heading IDs" do
        md = described_class.new("# Résumé\n\n## Résumé\n", options: {toc: true})
        toc_md = md.toc.to_markdown
        html = md.to_html
        expect(toc_md).to include("#resume")
        expect(toc_md).to include("#resume-1")
        expect(html).to include('id="resume"')
        expect(html).to include('id="resume-1"')
      end

      it "memoizes the Toc object across calls" do
        md = described_class.new(source, options: {toc: true})
        expect(md.toc.object_id).to eq(md.toc.object_id)
      end
    end

    context "with toc: Hash form" do
      let(:multi_level) do
        "# H1\n\n## H2a\n\n### H3a\n\n## H2b\n\n#### H4\n"
      end

      it "treats `toc: {}` as enabled with no depth limit" do
        md = described_class.new(multi_level, options: {toc: {}})
        toc_md = md.toc.to_markdown
        expect(toc_md).to include("[H1]")
        expect(toc_md).to include("[H2a]")
        expect(toc_md).to include("[H3a]")
        expect(toc_md).to include("[H4]")
      end

      it "filters the TOC by level when depth is set" do
        md = described_class.new(multi_level, options: {toc: {depth: 2}})
        toc_md = md.toc.to_markdown
        expect(toc_md).to include("[H1]")
        expect(toc_md).to include("[H2a]")
        expect(toc_md).not_to include("[H3a]")
        expect(toc_md).not_to include("[H4]")
      end

      it "filters the HTML TOC by level too" do
        md = described_class.new(multi_level, options: {toc: {depth: 2}})
        html = md.toc.to_html
        expect(html).to include(">H2a</a>")
        expect(html).not_to include(">H3a</a>")
      end

      it "does not hide headings from extracts or statistics" do
        md = described_class.new(multi_level, options: {
          toc: {depth: 2},
          statistics: true,
          extract: {headings: true}
        })
        # All 5 headings still counted and extracted.
        expect(md.statistics[:heading_count]).to eq(5)
        expect(md.extracts[:headings].size).to eq(5)
      end

      it "raises on unknown keys in the toc Hash" do
        expect {
          described_class.new("# x", options: {toc: {zepth: 2}})
        }.to raise_error(ArgumentError, /unknown toc key/)
      end

      it "raises on depth outside 1..6" do
        expect {
          described_class.new("# x", options: {toc: {depth: 0}})
        }.to raise_error(ArgumentError, /Integer 1\.\.6/)
        expect {
          described_class.new("# x", options: {toc: {depth: 7}})
        }.to raise_error(ArgumentError, /Integer 1\.\.6/)
      end

      it "raises on non-Integer depth" do
        expect {
          described_class.new("# x", options: {toc: {depth: "2"}})
        }.to raise_error(ArgumentError, /Integer 1\.\.6/)
      end

      it "accepts nil depth as 'no limit'" do
        md = described_class.new(multi_level, options: {toc: {depth: nil}})
        expect(md.toc.to_markdown).to include("[H4]")
      end
    end

    context "with toc: false (default)" do
      it "returns nil from #toc" do
        expect(described_class.new(source).toc).to be_nil
      end
    end
  end

  describe "statistics" do
    let(:source) do
      <<~MD
        # Hello World

        This is a paragraph with [a link](https://example.net) and an
        image: ![cat](cat.png "fluffy").

        ## Code Example

        ```ruby
        puts "hello"
        ```

        More text here. Another [link](/local).

        A footnote reference[^note].

        [^note]: The body of the footnote.
      MD
    end

    context "with statistics: true" do
      it "returns a hash with word count" do
        md = described_class.new(source, options: {statistics: true})
        stats = md.statistics
        expect(stats[:word_count]).to be > 10
      end

      it "returns a hash with character count" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:character_count]).to be > 50
      end

      it "detects the likely language" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:likely_language]).to eq("eng")
      end

      it "includes language confidence" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:language_confidence]).to be > 0.5
      end

      it "counts headings" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:heading_count]).to eq(2)
      end

      it "counts code blocks" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:code_block_count]).to eq(1)
      end

      it "includes code-block contents in word_count" do
        src = "# Title\n\nProse word.\n\n```ruby\ncode tokens here\n```\n"
        md = described_class.new(src, options: {statistics: true})
        # "Title" + "Prose word" + "code tokens here" = 6
        expect(md.statistics[:word_count]).to eq(6)
      end

      it "counts images" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:image_count]).to eq(1)
      end

      it "counts links" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:link_count]).to eq(2)
      end

      it "counts footnote definitions" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.statistics[:footnote_definition_count]).to eq(1)
      end

      it "does not include structured arrays (those are in #extracts)" do
        md = described_class.new(source, options: {statistics: true})
        stats = md.statistics
        expect(stats).not_to have_key(:images)
        expect(stats).not_to have_key(:links)
        expect(stats).not_to have_key(:code_blocks)
        expect(stats).not_to have_key(:headings)
        expect(stats).not_to have_key(:footnote_definitions)
      end

      it "implies toc: TOC is available" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.toc.to_markdown).to include("[Hello World]")
        expect(md.toc.to_html).to include('<a href="#hello-world">')
      end

      it "implies headings: { ids: true }: rendered HTML has ids" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.to_html).to include('id="hello-world"')
      end
    end

    context "with toc: true only (no statistics)" do
      it "returns a lightweight stats hash with heading_count" do
        md = described_class.new(source, options: {toc: true})
        stats = md.statistics
        expect(stats[:heading_count]).to eq(2)
      end

      it "does not include word count in the lightweight hash" do
        md = described_class.new(source, options: {toc: true})
        expect(md.statistics.key?(:word_count)).to be false
      end
    end

    context "with statistics: false (default)" do
      it "returns nil" do
        expect(described_class.new(source).statistics).to be_nil
      end
    end
  end

  describe "extracts" do
    let(:source) do
      <<~MD
        # Hello World

        This is a paragraph with [a link](https://example.net) and an
        image: ![cat](cat.png "fluffy").

        ## Code Example

        ```ruby
        puts "hello"
        ```

        More text here. Another [link](/local).

        A footnote reference[^note].

        [^note]: The body of the footnote.
      MD
    end

    context "with nothing requested" do
      it "returns nil" do
        expect(described_class.new(source).extracts).to be_nil
      end

      it "returns nil when only statistics: true is set (no extract)" do
        md = described_class.new(source, options: {statistics: true})
        expect(md.extracts).to be_nil
      end
    end

    context "with extract: { images: true }" do
      it "returns only the :images key" do
        md = described_class.new(source, options: {extract: {images: true}})
        expect(md.extracts.keys).to eq([:images])
      end

      it "populates image records with src, alt, title, byte_range" do
        md = described_class.new(source, options: {extract: {images: true}})
        img = md.extracts[:images].first
        expect(img[:src]).to eq("cat.png")
        expect(img[:alt]).to eq("cat")
        expect(img[:title]).to eq("fluffy")
        expect(img[:byte_range]).to be_a(Range)
        expect(source.byteslice(img[:byte_range].begin, img[:byte_range].size)).to include("cat.png")
      end
    end

    context "with extract: { links: true }" do
      it "populates link records in document order with byte_range" do
        md = described_class.new(source, options: {extract: {links: true}})
        links = md.extracts[:links]
        expect(links.size).to eq(2)
        expect(links.map { |l| l[:href] }).to eq(["https://example.net", "/local"])
        expect(links.first[:byte_range]).to be_a(Range)
      end
    end

    context "with extract: { code_blocks: true }" do
      it "captures pre-parsed source, fence lang, and byte_range" do
        md = described_class.new(source, options: {extract: {code_blocks: true}})
        blk = md.extracts[:code_blocks].first
        expect(blk[:lang]).to eq("ruby")
        expect(blk[:source]).to eq(%(puts "hello"\n))
        expect(source.byteslice(blk[:byte_range].begin, blk[:byte_range].size)).to include("puts \"hello\"")
      end

      it "captures pre-filter source even when syntax_highlight is enabled" do
        md = described_class.new(source, options: {extract: {code_blocks: true}, syntax_highlight: true})
        expect(md.extracts[:code_blocks].first[:source]).to eq(%(puts "hello"\n))
      end

      it "captures indented code blocks with an empty lang" do
        src = "Intro paragraph.\n\n    plain indented line\n    second line\n"
        md = described_class.new(src, options: {extract: {code_blocks: true}})
        blk = md.extracts[:code_blocks].first
        expect(blk[:lang]).to eq("")
        expect(blk[:source]).to include("plain indented line")
      end
    end

    context "with extract: { headings: true }" do
      it "populates level, text, id, and byte_range" do
        md = described_class.new(source, options: {extract: {headings: true}})
        headings = md.extracts[:headings]
        expect(headings.size).to eq(2)
        expect(headings.first).to include(level: 1, text: "Hello World", id: "hello-world")
        expect(headings.first[:byte_range]).to be_a(Range)
      end

      it "auto-enables toc so md.toc is populated" do
        md = described_class.new(source, options: {extract: {headings: true}})
        expect(md.toc.to_markdown).to include("[Hello World]")
      end
    end

    context "with extract: { footnote_definitions: true }" do
      it "populates label, body text, and byte_range" do
        md = described_class.new(source, options: {extract: {footnote_definitions: true}})
        defs = md.extracts[:footnote_definitions]
        expect(defs.size).to eq(1)
        expect(defs.first[:label]).to eq("note")
        expect(defs.first[:text]).to eq("The body of the footnote.")
        expect(defs.first[:byte_range]).to be_a(Range)
      end
    end

    context "mutual trigger: toc: true" do
      it "auto-populates extracts[:headings]" do
        md = described_class.new(source, options: {toc: true})
        expect(md.extracts[:headings].size).to eq(2)
      end
    end

    context "validation" do
      it "rejects unknown extract kinds" do
        expect {
          described_class.new(source, options: {extract: {imgaes: true}})
        }.to raise_error(ArgumentError, /unknown extract kind: :imgaes/)
      end

      it "rejects non-boolean values" do
        expect {
          described_class.new(source, options: {extract: {images: "yes"}})
        }.to raise_error(ArgumentError, /invalid value for extract\[:images\]/)
      end

      it "accepts nil as a disabled state" do
        expect {
          described_class.new(source, options: {extract: nil})
        }.not_to raise_error
      end
    end
  end

  describe "auto-linking" do
    context "with links: { autolink: true }" do
      it "links a bare https URL" do
        html = described_class.to_html(
          "Visit https://example.net today",
          options: {links: {autolink: true}}
        )
        expect(html).to include('<a href="https://example.net">')
        expect(html).to include("https://example.net</a>")
      end

      it "links a bare http URL" do
        html = described_class.to_html(
          "See http://example.net for details",
          options: {links: {autolink: true}}
        )
        expect(html).to include('<a href="http://example.net">')
      end

      it "links a bare email as mailto:" do
        html = described_class.to_html(
          "Contact user@example.net",
          options: {links: {autolink: true}}
        )
        expect(html).to include('href="mailto:user@example.net"')
      end

      it "preserves text inside existing links" do
        html = described_class.to_html(
          "[click](https://example.net)",
          options: {links: {autolink: true}}
        )
        # Should have exactly one <a>, not a nested link.
        expect(html.scan("<a ").size).to eq(1)
      end

      it "preserves URLs inside fenced code blocks" do
        html = described_class.to_html(
          "```\nhttps://example.net\n```",
          options: {links: {autolink: true}}
        )
        expect(html).not_to include("<a href=")
      end

      it "preserves URLs inside inline code" do
        html = described_class.to_html(
          "Use `https://example.net` in code",
          options: {links: {autolink: true}}
        )
        expect(html).not_to include('<a href="https://example.net">')
      end

      it "handles trailing punctuation correctly" do
        html = described_class.to_html(
          "Check https://example.net.",
          options: {links: {autolink: true}}
        )
        # The trailing period should NOT be part of the link.
        expect(html).to include("https://example.net</a>.")
      end
    end

    context "with links: { autolink: false } (default)" do
      it "does not link bare URLs" do
        html = described_class.to_html("Visit https://example.net")
        expect(html).not_to include("<a ")
      end
    end
  end

  describe "syntax highlighting" do
    context "with syntax_highlight: true" do
      it "adds span tags to fenced code blocks with a language" do
        html = described_class.to_html("```rust\nlet x = 1;\n```", options: {syntax_highlight: true})
        expect(html).to include("<span")
        expect(html).to include("language-rust")
      end

      it "wraps highlighted code in pre>code" do
        html = described_class.to_html("```ruby\nputs 'hi'\n```", options: {syntax_highlight: true})
        expect(html).to include("<pre><code")
        expect(html).to include("</code></pre>")
      end

      it "leaves code blocks without a language unhighlighted" do
        html = described_class.to_html("```\nplain code\n```", options: {syntax_highlight: true})
        expect(html).not_to include("<span class=")
      end

      it "leaves indented code blocks unhighlighted" do
        html = described_class.to_html("    indented code\n", options: {syntax_highlight: true})
        expect(html).not_to include("<span class=")
      end

      it "falls back gracefully for unknown languages" do
        html = described_class.to_html("```nonexistent_xyz\nhello\n```", options: {syntax_highlight: true})
        expect(html).to include("hello")
        expect(html).to include("<pre><code")
      end
    end

    context "with syntax_highlight: false (default)" do
      it "does not add span tags" do
        html = described_class.to_html("```rust\nlet x = 1;\n```")
        expect(html).not_to include("<span class=")
      end
    end
  end

  describe ".highlight_css" do
    it "returns a CSS string" do
      css = described_class.highlight_css
      expect(css).to include("color:")
      expect(css).to include("background-color:")
    end

    it "accepts a theme: keyword" do
      themes = described_class.highlight_themes
      css = described_class.highlight_css(theme: themes.first)
      expect(css).to include("color:")
    end

    it "raises ArgumentError for unknown themes" do
      expect { described_class.highlight_css(theme: "nonexistent_theme_xyz") }
        .to raise_error(ArgumentError, /unknown syntax theme/)
    end
  end

  describe ".highlight_themes" do
    it "returns an array of theme names" do
      themes = described_class.highlight_themes
      expect(themes).to be_an(Array)
      expect(themes.size).to be > 3
      expect(themes).to include("base16-ocean.dark")
    end
  end

  describe "emoji shortcodes" do
    context "with emoji_shortcodes: true" do
      it "replaces a known shortcode with the emoji character" do
        html = described_class.to_html("Ship it! :rocket:", options: {emoji_shortcodes: true})
        expect(html).to include("🚀")
        expect(html).not_to include(":rocket:")
      end

      it "replaces multiple shortcodes in one paragraph" do
        html = described_class.to_html(":tada: :rocket: :100:", options: {emoji_shortcodes: true})
        expect(html).to include("🎉")
        expect(html).to include("🚀")
        expect(html).to include("💯")
      end

      it "leaves unknown shortcodes as literal text" do
        html = described_class.to_html(":not_a_real_emoji:", options: {emoji_shortcodes: true})
        expect(html).to include(":not_a_real_emoji:")
      end

      it "preserves shortcodes inside fenced code blocks" do
        source = "```\nShip it! :rocket:\n```\n"
        html = described_class.to_html(source, options: {emoji_shortcodes: true})
        expect(html).to include(":rocket:")
        expect(html).not_to include("🚀")
      end

      it "preserves shortcodes inside inline code spans" do
        html = described_class.to_html("Use `:rocket:` for rockets", options: {emoji_shortcodes: true})
        expect(html).to include(":rocket:")
        expect(html).not_to include("🚀")
      end

      it "does not match case-variant shortcodes" do
        html = described_class.to_html(":Rocket:", options: {emoji_shortcodes: true})
        expect(html).to include(":Rocket:")
        expect(html).not_to include("🚀")
      end

      it "composes with headings: { ids: true } so the slug reflects the emoji" do
        html = described_class.to_html(
          "# :rocket: Launch",
          options: {emoji_shortcodes: true, headings: {ids: true}}
        )
        # Pipeline order: emoji filter replaces :rocket: with 🚀, then
        # headings: { ids: true } runs slugify on "🚀 Launch". deunicode knows that
        # 🚀 transliterates to "rocket", so the final slug surfaces the
        # emoji's English name rather than dropping it.
        expect(html).to include('id="rocket-launch"')
        expect(html).to include("🚀 Launch")
      end
    end

    context "with emoji_shortcodes: false (default)" do
      it "leaves shortcodes as literal text" do
        html = described_class.to_html(":rocket:")
        expect(html).to include(":rocket:")
        expect(html).not_to include("🚀")
      end
    end
  end

  describe "lazy images" do
    context "with images: { lazy: true }" do
      it "adds loading and decoding attributes to a basic image" do
        html = described_class.to_html("![cat](cat.png)", options: {images: {lazy: true}})
        expect(html).to include('<img src="cat.png"')
        expect(html).to include('alt="cat"')
        expect(html).to include('loading="lazy"')
        expect(html).to include('decoding="async"')
      end

      it "preserves the title attribute when present" do
        html = described_class.to_html('![cat](cat.png "fluffy")', options: {images: {lazy: true}})
        expect(html).to include('title="fluffy"')
        expect(html).to include('loading="lazy"')
      end

      it "escapes HTML specials in alt text" do
        html = described_class.to_html('![a"b<c>d&e](img.png)', options: {images: {lazy: true}})
        expect(html).to include("&quot;")
        expect(html).to include("&lt;")
        expect(html).to include("&gt;")
        expect(html).to include("&amp;")
        expect(html).not_to include('alt="a"b<c>d&e"')
      end

      it "escapes ampersands in image URLs" do
        html = described_class.to_html("![](img.png?a=1&b=2)", options: {images: {lazy: true}})
        expect(html).to include("img.png?a=1&amp;b=2")
      end

      it "handles multiple images in one document" do
        html = described_class.to_html(
          "![one](a.png)\n\n![two](b.png)",
          options: {images: {lazy: true}}
        )
        expect(html.scan('loading="lazy"').size).to eq(2)
      end

      it "works inside links" do
        html = described_class.to_html(
          "[![cat](cat.png)](https://example.net)",
          options: {images: {lazy: true}}
        )
        expect(html).to include('<a href="https://example.net">')
        expect(html).to include('loading="lazy"')
        expect(html).to include("</a>")
      end
    end

    context "with images: { lazy: false } (default)" do
      it "does not add loading or decoding attributes" do
        html = described_class.to_html("![cat](cat.png)")
        expect(html).not_to include('loading="lazy"')
        expect(html).not_to include('decoding="async"')
      end
    end
  end

  describe "external link rel" do
    context "with links: { nofollow: true }" do
      it "adds rel=nofollow noopener to https links" do
        html = described_class.to_html(
          "[docs](https://example.net)",
          options: {links: {nofollow: true}}
        )
        expect(html).to include('<a href="https://example.net" rel="nofollow noopener">docs</a>')
      end

      it "adds rel to http links too" do
        html = described_class.to_html(
          "[insecure](http://example.net)",
          options: {links: {nofollow: true}}
        )
        expect(html).to include('rel="nofollow noopener"')
      end

      it "is case-insensitive on the scheme" do
        html = described_class.to_html(
          "[mixed](HTTPS://example.net)",
          options: {links: {nofollow: true}}
        )
        expect(html).to include('rel="nofollow noopener"')
      end

      it "leaves relative links unchanged" do
        html = described_class.to_html(
          "[home](/home)",
          options: {links: {nofollow: true}}
        )
        expect(html).to include('<a href="/home">home</a>')
        expect(html).not_to include("rel=")
      end

      it "leaves anchor fragments unchanged" do
        html = described_class.to_html(
          "[top](#intro)",
          options: {links: {nofollow: true}}
        )
        expect(html).not_to include("rel=")
      end

      it "leaves mailto: links unchanged" do
        html = described_class.to_html(
          "[email](mailto:user@example.net)",
          options: {links: {nofollow: true}}
        )
        expect(html).not_to include("rel=")
      end

      it "preserves the title attribute on external links" do
        html = described_class.to_html(
          '[docs](https://example.net "the docs")',
          options: {links: {nofollow: true}}
        )
        expect(html).to include('title="the docs"')
        expect(html).to include('rel="nofollow noopener"')
      end

      it "preserves inline formatting inside the link text" do
        html = described_class.to_html(
          "[**bold** link](https://example.net)",
          options: {links: {nofollow: true}}
        )
        expect(html).to include("<strong>bold</strong>")
        expect(html).to include('rel="nofollow noopener"')
      end

      it "handles multiple external links in one document" do
        source = "[one](https://a.com) and [two](https://b.com) and [three](/local)"
        html = described_class.to_html(source, options: {links: {nofollow: true}})
        expect(html.scan('rel="nofollow noopener"').size).to eq(2)
      end
    end

    context "with links: { nofollow: false } (default)" do
      it "does not add rel attributes" do
        html = described_class.to_html("[docs](https://example.net)")
        expect(html).not_to include("rel=")
      end
    end
  end

  describe "raw HTML handling" do
    context "by default" do
      it "escapes raw block-level HTML" do
        html = described_class.to_html("<script>alert(1)</script>")
        expect(html).to include("&lt;script&gt;")
        expect(html).not_to include("<script>")
      end

      it "escapes inline raw HTML" do
        html = described_class.to_html("hi <em>raw</em> there")
        expect(html).to include("&lt;em&gt;")
        expect(html).not_to include("<em>raw</em>")
      end
    end

    context "when raw_html is true" do
      it "passes raw HTML through unchanged" do
        html = described_class.to_html("<em>raw</em>", options: {raw_html: true})
        expect(html).to include("<em>raw</em>")
      end

      it "passes block-level raw HTML through unchanged" do
        html = described_class.to_html("<div>content</div>", options: {raw_html: true})
        expect(html).to include("<div>content</div>")
      end
    end
  end

  describe "GFM tagfilter" do
    # GFM §6.11 "Disallowed Raw HTML". Mirrors comrak/cmark-gfm —
    # escapes the leading '<' of nine unsafe tag names.
    let(:disallowed) { %w[title textarea style xmp iframe noembed noframes script plaintext] }

    context "when gfm: true and raw_html: true (defaults)" do
      it "escapes all nine disallowed tag names" do
        disallowed.each do |tag|
          html = described_class.to_html("<#{tag}>payload", options: {raw_html: true})
          expect(html).to include("&lt;#{tag}>"), "tag: #{tag}"
        end
      end

      it "escapes closing disallowed tags" do
        html = described_class.to_html("</script>", options: {raw_html: true})
        expect(html).to include("&lt;/script>")
      end

      it "is case-insensitive" do
        html = described_class.to_html("<SCRIPT>", options: {raw_html: true})
        expect(html).to include("&lt;SCRIPT>")
      end

      it "does not match tag-name prefixes" do
        html = described_class.to_html("<scripter>", options: {raw_html: true})
        expect(html).to include("<scripter>")
        expect(html).not_to include("&lt;scripter>")
      end

      it "leaves safe tags untouched" do
        html = described_class.to_html("<b>ok</b>", options: {raw_html: true})
        expect(html).to include("<b>ok</b>")
      end
    end

    context "when gfm_tag_filter is explicitly disabled" do
      it "passes the disallowed tag through unchanged" do
        html = described_class.to_html(
          "<script>alert(1)</script>",
          options: {raw_html: true, gfm_tag_filter: false}
        )
        expect(html).to include("<script>alert(1)</script>")
        expect(html).not_to include("&lt;script>")
      end
    end

    context "when gfm is disabled" do
      it "does not run the tagfilter (non-GFM mode)" do
        html = described_class.to_html(
          "<script>alert(1)</script>",
          options: {raw_html: true, gfm: false}
        )
        expect(html).to include("<script>alert(1)</script>")
      end
    end

    context "when raw_html is false (default)" do
      it "is moot—suppress_raw_html already escapes everything" do
        html = described_class.to_html("<script>alert(1)</script>")
        # Fully escaped via suppress_raw_html, not double-escaped via tagfilter.
        expect(html).to include("&lt;script&gt;alert(1)&lt;/script&gt;")
        expect(html).not_to include("&amp;lt;")
      end
    end
  end

  describe "thread safety" do
    it "renders the same input identically from many threads" do
      g = described_class.new("# hello")
      results = 8.times.map { Thread.new { g.to_html } }.map(&:value)
      expect(results.uniq).to eq(["<h1>hello</h1>\n"])
    end

    it "renders different inputs concurrently without corruption" do
      inputs = ["# one", "# two", "# three", "# four"]
      results = inputs.map { |md| Thread.new { described_class.to_html(md) } }.map(&:value)
      expected = inputs.map { |md| "<h1>#{md[2..]}</h1>\n" }
      expect(results).to eq(expected)
    end
  end

  describe "hard_wrap" do
    it "renders a single newline as <br /> when enabled" do
      html = described_class.to_html("line one\nline two", options: {hard_wrap: true})
      expect(html).to include("<br />")
    end

    it "renders a single newline as a space by default" do
      html = described_class.to_html("line one\nline two")
      expect(html).not_to include("<br />")
    end
  end

  describe "#to_markdown" do
    it "round-trips plain markdown" do
      expect(described_class.new("**bold**").to_markdown).to eq("**bold**")
    end

    it "returns an empty string for empty source" do
      expect(described_class.new("").to_markdown).to eq("")
    end

    it "expands emoji shortcodes in the markdown output" do
      g = described_class.new(":rocket: launch", options: {emoji_shortcodes: true})
      expect(g.to_markdown).to eq("🚀 launch")
    end

    it "unwraps disallowed links to plain text" do
      g = described_class.new("[evil](https://evil.com)", options: {links: {allowed_hosts: []}})
      expect(g.to_markdown).to eq("evil")
    end

    it "embeds HTML when syntax_highlight is on" do
      md = "```ruby\nx = 1\n```"
      g = described_class.new(md, options: {syntax_highlight: true})
      expect(g.to_markdown).to include("<pre")
    end
  end

  describe ".to_markdown" do
    it "returns filtered markdown" do
      expect(described_class.to_markdown(":+1:", options: {emoji_shortcodes: true}))
        .to eq("👍")
    end

    it "returns an empty string for nil" do
      expect(described_class.to_markdown(nil)).to eq("")
    end
  end

  describe "#to_plain_text" do
    it "strips emphasis, keeping inner text" do
      expect(described_class.new("**bold** and *italic*").to_plain_text)
        .to eq("bold and italic\n")
    end

    it "expands links to text (url)" do
      expect(described_class.new("[example](https://example.net)").to_plain_text)
        .to eq("example (https://example.net)\n")
    end

    it "collapses autolinks where text equals url" do
      expect(described_class.new("<https://example.net>").to_plain_text)
        .to eq("https://example.net\n")
    end

    it "expands images to alt (src)" do
      expect(described_class.new("![cat](cat.png)").to_plain_text)
        .to eq("cat (cat.png)\n")
    end

    it "renders headings as plain text with blank-line separation" do
      expect(described_class.new("# Title\n\nbody").to_plain_text)
        .to eq("Title\n\nbody\n")
    end

    it "prefixes blockquote lines with '> '" do
      expect(described_class.new("> quoted").to_plain_text).to eq("> quoted\n")
    end

    it "nests blockquote prefixes" do
      expect(described_class.new("> > deep").to_plain_text).to eq("> > deep\n")
    end

    it "uses bare '>' for blank lines inside a blockquote" do
      md = "> first\n>\n> second"
      expect(described_class.new(md).to_plain_text).to eq("> first\n>\n> second\n")
    end

    it "bullets unordered list items with '- '" do
      expect(described_class.new("- a\n- b").to_plain_text).to eq("- a\n- b\n")
    end

    it "indents nested list items by two spaces" do
      expect(described_class.new("- outer\n  - inner").to_plain_text)
        .to eq("- outer\n  - inner\n")
    end

    it "drops tasklist checkboxes" do
      expect(described_class.new("- [x] done\n- [ ] todo", options: {tasklists: true}).to_plain_text)
        .to eq("- done\n- todo\n")
    end

    it "tab-separates table cells with a blank line after the header" do
      md = "| a | b |\n|---|---|\n| 1 | 2 |\n"
      expect(described_class.new(md).to_plain_text).to eq("a\tb\n\n1\t2\n")
    end

    it "preserves code block contents verbatim" do
      expect(described_class.new("```ruby\nputs :hi\n```").to_plain_text)
        .to eq("puts :hi\n")
    end

    it "emits --- for horizontal rules" do
      expect(described_class.new("a\n\n---\n\nb").to_plain_text)
        .to eq("a\n\n---\n\nb\n")
    end

    it "renders footnote refs as [label] with definitions appended" do
      md = "Body[^x].\n\n[^x]: the note"
      expect(described_class.new(md).to_plain_text)
        .to eq("Body[x].\n\n[x]: the note\n")
    end

    it "applies emoji replacement in plain text when enabled" do
      g = described_class.new(":rocket: launch", options: {emoji_shortcodes: true})
      expect(g.to_plain_text).to eq("🚀 launch\n")
    end

    it "unwraps disallowed links via host allowlist" do
      g = described_class.new("[evil](https://evil.com)", options: {links: {allowed_hosts: []}})
      expect(g.to_plain_text).to eq("evil\n")
    end

    it "returns an empty string for empty source" do
      expect(described_class.new("").to_plain_text).to eq("")
    end
  end

  describe ".truncate_markdown" do
    let(:doc) do
      <<~MD
        First paragraph has some prose words here.

        Second paragraph with more content, including a longer phrase.

        Third paragraph wraps things up with a concluding thought.
      MD
    end

    context "validation" do
      it "raises when neither chars nor words is given" do
        expect { described_class.truncate_markdown(doc, at: :block) }
          .to raise_error(ArgumentError, /at least one of :chars or :words/)
      end

      it "raises for an invalid at: value" do
        expect { described_class.truncate_markdown(doc, chars: 100, at: :sentence) }
          .to raise_error(ArgumentError, /:at must be :block or :word/)
      end

      it "raises for a non-Integer char limit" do
        expect { described_class.truncate_markdown(doc, chars: "100") }
          .to raise_error(ArgumentError, /:chars must be an Integer/)
      end

      it "raises when the marker is larger than the char budget" do
        expect { described_class.truncate_markdown(doc, chars: 3, marker: "[truncated]") }
          .to raise_error(ArgumentError, /:marker .* must be shorter/)
      end

      it "raises for a non-String marker" do
        expect { described_class.truncate_markdown(doc, chars: 100, marker: 42) }
          .to raise_error(ArgumentError, /:marker must be a String or nil/)
      end
    end

    context "block mode" do
      it "returns the content unchanged (no marker) when it fits" do
        short = "One small paragraph.\n"
        result = described_class.truncate_markdown(short, chars: 1000, at: :block)
        expect(result).to include("One small paragraph")
        expect(result).not_to include("…")
      end

      it "truncates at a block boundary with default ellipsis marker" do
        result = described_class.truncate_markdown(doc, chars: 60, at: :block)
        expect(result).to include("First paragraph")
        expect(result).not_to include("Third paragraph")
        expect(result).to end_with("…\n")
      end

      it "suppresses marker when nil" do
        result = described_class.truncate_markdown(doc, chars: 60, at: :block, marker: nil)
        expect(result).not_to include("…")
      end

      it "uses a custom marker" do
        result = described_class.truncate_markdown(doc, chars: 80, at: :block, marker: "[…]")
        expect(result).to include("[…]")
      end

      it "returns empty when the first block alone exceeds the budget" do
        long_para = "a " * 500 + "\n"
        expect(described_class.truncate_markdown(long_para, chars: 50, at: :block))
          .to eq("")
      end

      it "preserves valid Markdown structure" do
        md = "# Title\n\n```ruby\ncode\n```\n\nMore text here.\n"
        result = described_class.truncate_markdown(md, chars: 30, at: :block)
        # A code block either fits whole or is dropped — never half-cut.
        expect(result).not_to include("```ruby\ncode") if !result.include?("```\n")
      end
    end

    context "word mode" do
      it "truncates at a word boundary with default marker" do
        result = described_class.truncate_markdown(doc, chars: 30, at: :word)
        expect(result.chars.count).to be <= 30
        expect(result).to end_with("…")
      end

      it "respects a word-count budget" do
        result = described_class.truncate_markdown(doc, words: 5, at: :word)
        # 5 words of content plus the marker
        expect(result.scan(/\p{Word}+/).count).to be <= 5
      end

      it "honors both chars and words, cutting at whichever comes first" do
        # chars budget is loose (1000), words budget is tight (3)
        result = described_class.truncate_markdown(doc, chars: 1000, words: 3, at: :word)
        expect(result.scan(/\p{Word}+/).count).to be <= 3
      end
    end

    context "filter-applied content" do
      it "truncates AFTER emoji expansion so counts reflect the embedded output" do
        src = ":rocket: :rocket: :rocket: :rocket: :rocket:\n"
        result = described_class.truncate_markdown(
          src, chars: 6, at: :word, options: {emoji_shortcodes: true}
        )
        # After emoji expansion, each :rocket: → 🚀 (1 char). With budget 6,
        # we can fit ~5 emoji + marker.
        expect(result).to include("🚀")
        expect(result).not_to include(":rocket:")
      end
    end

    context "empty/nil source" do
      it "returns empty string for nil" do
        expect(described_class.truncate_markdown(nil, chars: 100)).to eq("")
      end

      it "returns empty string for empty source" do
        expect(described_class.truncate_markdown("", chars: 100)).to eq("")
      end
    end
  end

  describe "#truncate_markdown" do
    it "uses stored options" do
      g = described_class.new(":rocket: launch!", options: {emoji_shortcodes: true})
      result = g.truncate_markdown(chars: 100, at: :word)
      expect(result).to include("🚀")
    end
  end

  describe ".chunks_by_heading with truncate:" do
    let(:doc) do
      <<~MD
        # Intro

        Short intro.

        ## Details

        This section has a longer body with multiple sentences covering various points.

        ## Summary

        Final thoughts go here in a short paragraph.
      MD
    end

    it "truncates each section's content to the budget" do
      sections = described_class.chunks_by_heading(
        doc, truncate: {chars: 40, at: :block}
      )
      sections.each do |s|
        expect(s[:content].chars.count).to be <= 40
      end
    end

    it "does not affect metadata (heading, level, id, breadcrumb)" do
      sections = described_class.chunks_by_heading(
        doc, truncate: {chars: 40, at: :block}
      )
      details = sections.find { |s| s[:heading] == "Details" }
      expect(details[:level]).to eq(2)
      expect(details[:id]).to eq("details")
      expect(details[:breadcrumb]).to eq(["Intro"])
    end

    it "recomputes counts on the truncated content when statistics is set" do
      sections = described_class.chunks_by_heading(
        doc,
        options: {statistics: true},
        truncate: {chars: 40, at: :block}
      )
      sections.each do |s|
        # word_count of the truncated content is bounded by its char count
        expect(s[:word_count]).to be <= s[:content].scan(/\p{Word}+/).count + 1
      end
    end

    it "truncates the preamble too" do
      src = "This is a reasonably long preamble paragraph. More text here.\n\n# A\n\nbody\n"
      sections = described_class.chunks_by_heading(src, truncate: {chars: 30, at: :block})
      preamble = sections.first
      expect(preamble[:heading]).to be_nil
      expect(preamble[:content].chars.count).to be <= 30
    end
  end

  describe ".to_plain_text" do
    it "works via the class-method shortcut" do
      expect(described_class.to_plain_text("**hi** [link](https://x.com)"))
        .to eq("hi link (https://x.com)\n")
    end

    it "returns an empty string for nil" do
      expect(described_class.to_plain_text(nil)).to eq("")
    end
  end

  describe ".chunks_by_heading" do
    let(:doc) do
      <<~MD
        This is the preamble.

        # Intro

        Top intro text.

        ## Installation

        Run `gem install inkmark`.

        ## Usage

        Call `Inkmark.to_html`.

        ### Advanced

        Advanced usage notes.

        ## Licensing

        MIT.
      MD
    end

    it "returns every section plus the preamble" do
      sections = described_class.chunks_by_heading(doc)
      expect(sections.map { |s| s[:heading] })
        .to eq([nil, "Intro", "Installation", "Usage", "Advanced", "Licensing"])
    end

    it "marks preamble with level 0 and nil id" do
      preamble = described_class.chunks_by_heading(doc).first
      expect(preamble).to include(heading: nil, level: 0, id: nil)
      expect(preamble[:content]).to include("This is the preamble.")
    end

    it "assigns correct heading level to each section" do
      sections = described_class.chunks_by_heading(doc)
      levels = sections.map { |s| [s[:heading], s[:level]] }
      expect(levels).to include(["Intro", 1], ["Installation", 2], ["Advanced", 3])
    end

    it "generates slug ids matching headings: { ids: true } convention" do
      sections = described_class.chunks_by_heading(doc)
      install = sections.find { |s| s[:heading] == "Installation" }
      expect(install[:id]).to eq("installation")
    end

    it "includes nested subsections in parent section content (hierarchical)" do
      sections = described_class.chunks_by_heading(doc)
      usage = sections.find { |s| s[:heading] == "Usage" }
      expect(usage[:content]).to include("Call `Inkmark.to_html`")
      expect(usage[:content]).to include("Advanced usage notes")
    end

    it "also emits the nested subsection as its own entry" do
      sections = described_class.chunks_by_heading(doc)
      advanced = sections.find { |s| s[:heading] == "Advanced" }
      expect(advanced[:level]).to eq(3)
      expect(advanced[:content]).to include("Advanced usage notes")
      expect(advanced[:content]).not_to include("Call `Inkmark.to_html`")
    end

    it "ends a section at the next equal-or-higher-level heading" do
      sections = described_class.chunks_by_heading(doc)
      usage = sections.find { |s| s[:heading] == "Usage" }
      expect(usage[:content]).not_to include("MIT")
    end

    context "breadcrumb" do
      it "is empty for root-level headings" do
        sections = described_class.chunks_by_heading(doc)
        intro = sections.find { |s| s[:heading] == "Intro" }
        expect(intro[:breadcrumb]).to eq([])
      end

      it "lists the root heading for a level-2 section" do
        sections = described_class.chunks_by_heading(doc)
        install = sections.find { |s| s[:heading] == "Installation" }
        expect(install[:breadcrumb]).to eq(["Intro"])
      end

      it "lists every ancestor for a deeply nested section" do
        sections = described_class.chunks_by_heading(doc)
        advanced = sections.find { |s| s[:heading] == "Advanced" }
        expect(advanced[:breadcrumb]).to eq(["Intro", "Usage"])
      end

      it "resets when a higher-level heading appears" do
        md = "# A\n\n## A1\n\n# B\n\n## B1\n"
        sections = described_class.chunks_by_heading(md)
        b1 = sections.find { |s| s[:heading] == "B1" }
        expect(b1[:breadcrumb]).to eq(["B"])
      end

      it "omits skipped levels (h3 directly under h1 has only h1)" do
        md = "# Top\n\n### Deep\n\nbody\n"
        sections = described_class.chunks_by_heading(md)
        deep = sections.find { |s| s[:heading] == "Deep" }
        expect(deep[:breadcrumb]).to eq(["Top"])
      end

      it "is empty for the preamble" do
        preamble = described_class.chunks_by_heading(doc).first
        expect(preamble[:heading]).to be_nil
        expect(preamble[:breadcrumb]).to eq([])
      end

      it "uses filter-applied heading text in ancestor entries" do
        md = "# :rocket: Launch\n\n## Details\n\nbody\n"
        sections = described_class.chunks_by_heading(md, options: {emoji_shortcodes: true})
        details = sections.find { |s| s[:heading] == "Details" }
        expect(details[:breadcrumb]).to eq(["🚀 Launch"])
      end
    end

    context "with statistics: true" do
      let(:stats_doc) do
        <<~MD
          Preamble sentence here.

          # Intro

          Two short words.

          ## Code

          Here is code:

          ```ruby
          puts "hi"
          ```

          More prose.
        MD
      end

      it "includes :character_count and :word_count on every section" do
        sections = described_class.chunks_by_heading(stats_doc, options: {statistics: true})
        sections.each do |s|
          expect(s).to include(:character_count, :word_count)
          expect(s[:character_count]).to be_a(Integer)
          expect(s[:word_count]).to be_a(Integer)
        end
      end

      it "counts words in a plain-prose section correctly" do
        sections = described_class.chunks_by_heading(stats_doc, options: {statistics: true})
        intro = sections.find { |s| s[:heading] == "Intro" }
        # "Two short words" = 3 words; hierarchical content includes the
        # nested Code section, so the total exceeds 3.
        expect(intro[:word_count]).to be >= 3
      end

      it "includes code-block contents in the count" do
        only_code = "# Top\n\n```ruby\none two three four\n```\n"
        sections = described_class.chunks_by_heading(only_code, options: {statistics: true})
        top = sections.first
        expect(top[:word_count]).to eq(4)
      end

      it "also annotates the preamble" do
        sections = described_class.chunks_by_heading(stats_doc, options: {statistics: true})
        preamble = sections.first
        expect(preamble[:heading]).to be_nil
        expect(preamble[:word_count]).to eq(3) # "Preamble sentence here"
      end

      it "omits the count keys when statistics is not set" do
        sections = described_class.chunks_by_heading(stats_doc)
        sections.each do |s|
          expect(s).not_to have_key(:character_count)
          expect(s).not_to have_key(:word_count)
        end
      end
    end

    context "filter-applied content" do
      it "expands emoji shortcodes inside heading and content" do
        md = "# :rocket: Launch\n\nUse :sparkles: everywhere.\n"
        sections = described_class.chunks_by_heading(md, options: {emoji_shortcodes: true})
        section = sections.first
        expect(section[:heading]).to eq("🚀 Launch")
        expect(section[:content]).to include("✨")
      end

      it "autolinks URLs in content when autolink is on" do
        md = "# Links\n\nVisit https://example.net now.\n"
        section = described_class.chunks_by_heading(md, options: {links: {autolink: true}}).first
        expect(section[:content]).to include("<https://example.net>")
      end

      it "unwraps disallowed links via host allowlist" do
        md = "# L\n\n[evil](https://evil.com)\n"
        section = described_class.chunks_by_heading(md, options: {links: {allowed_hosts: []}}).first
        expect(section[:content]).to include("evil")
        expect(section[:content]).not_to include("evil.com")
      end
    end

    context "edge cases" do
      it "returns an empty array for nil source" do
        expect(described_class.chunks_by_heading(nil)).to eq([])
      end

      it "returns an empty array for an empty string" do
        expect(described_class.chunks_by_heading("")).to eq([])
      end

      it "treats a no-heading document as a single preamble entry" do
        sections = described_class.chunks_by_heading("Just a paragraph.\n\nAnother one.\n")
        expect(sections.size).to eq(1)
        expect(sections.first).to include(heading: nil, level: 0, id: nil)
        expect(sections.first[:content]).to include("Just a paragraph.")
      end

      it "does not emit a preamble when the doc starts with a heading" do
        sections = described_class.chunks_by_heading("# First\n\nBody.\n")
        expect(sections.size).to eq(1)
        expect(sections.first[:heading]).to eq("First")
      end

      it "dedups ids for duplicate headings" do
        md = "## Same\n\none\n\n## Same\n\ntwo\n"
        sections = described_class.chunks_by_heading(md)
        ids = sections.map { |s| s[:id] }
        expect(ids).to eq(["same", "same-1"])
      end
    end
  end

  describe "#chunks_by_heading" do
    it "uses instance options" do
      md = "# :rocket: Go\n\nText."
      g = described_class.new(md, options: {emoji_shortcodes: true})
      expect(g.chunks_by_heading.first[:heading]).to eq("🚀 Go")
    end

    it "returns an empty array for an empty instance source" do
      expect(described_class.new("").chunks_by_heading).to eq([])
    end
  end

  describe ".chunks_by_size" do
    let(:doc) do
      # ~120 chars per paragraph, 6 paragraphs = ~720 chars.
      paragraphs = 6.times.map { |i| "This is paragraph number #{i}, containing several reasonably-sized words for testing purposes." }
      paragraphs.join("\n\n") + "\n"
    end

    context "validation" do
      it "raises when neither chars nor words is given" do
        expect { described_class.chunks_by_size(doc) }
          .to raise_error(ArgumentError, /at least one of :chars or :words/)
      end

      it "raises when chars is not a positive Integer" do
        expect { described_class.chunks_by_size(doc, chars: 0) }
          .to raise_error(ArgumentError, /:chars must be positive/)
        expect { described_class.chunks_by_size(doc, chars: "100") }
          .to raise_error(ArgumentError, /:chars must be an Integer/)
      end

      it "raises when overlap >= chars budget" do
        expect { described_class.chunks_by_size(doc, chars: 100, overlap: 100) }
          .to raise_error(ArgumentError, /:overlap .* must be less than :chars budget/)
        expect { described_class.chunks_by_size(doc, chars: 100, overlap: 200) }
          .to raise_error(ArgumentError, /:overlap .* must be less than :chars budget/)
      end

      it "raises on negative overlap" do
        expect { described_class.chunks_by_size(doc, chars: 100, overlap: -5) }
          .to raise_error(ArgumentError, /:overlap must be non-negative/)
      end

      it "raises on invalid at: value" do
        expect { described_class.chunks_by_size(doc, chars: 100, at: :sentence) }
          .to raise_error(ArgumentError, /:at must be :block or :word/)
      end
    end

    context "block mode" do
      it "produces multiple windows when the doc exceeds the budget" do
        windows = described_class.chunks_by_size(doc, chars: 300, at: :block)
        expect(windows.size).to be > 1
      end

      it "assigns sequential 0-based indices" do
        windows = described_class.chunks_by_size(doc, chars: 300, at: :block)
        expect(windows.map { |w| w[:index] }).to eq((0...windows.size).to_a)
      end

      it "keeps every window under the budget when the doc is decomposable" do
        windows = described_class.chunks_by_size(doc, chars: 300, at: :block)
        # The doc has 120-char paragraphs, budget 300 → each window ~2 paragraphs.
        windows.each { |w| expect(w[:content].chars.count).to be <= 350 }
      end

      it "emits oversized blocks as their own window (decision A)" do
        big_block = "a " * 1000  # 2000 chars in one paragraph
        windows = described_class.chunks_by_size(big_block, chars: 200, at: :block)
        expect(windows.size).to eq(1)
        expect(windows.first[:content].chars.count).to be > 200
      end

      it "respects a words budget" do
        # ~13 words per paragraph → 3 paragraphs ~= 39 words.
        windows = described_class.chunks_by_size(doc, words: 40, at: :block)
        windows.each { |w| expect(w[:content].scan(/\p{Word}+/).count).to be <= 50 }
      end

      it "applies overlap as a prefix of the next window" do
        windows = described_class.chunks_by_size(doc, chars: 300, overlap: 50, at: :block)
        # Each non-first window starts with at least some overlap content.
        # Naive check: total content chars > sum of chunks-minus-overlap.
        total = windows.sum { |w| w[:content].chars.count }
        expect(total).to be > doc.chars.count
      end

      it "returns just one window for a doc that fits" do
        short = "one short paragraph.\n"
        windows = described_class.chunks_by_size(short, chars: 1000, at: :block)
        expect(windows.size).to eq(1)
        expect(windows.first[:content]).to include("one short paragraph")
      end
    end

    context "word mode" do
      it "cuts at word boundaries" do
        windows = described_class.chunks_by_size(doc, chars: 150, at: :word)
        windows.each do |w|
          # No window ends mid-word (except possibly the last).
          expect(w[:content].chars.count).to be <= 160
        end
      end

      it "honors a words budget" do
        windows = described_class.chunks_by_size(doc, words: 10, at: :word)
        windows.each { |w| expect(w[:content].scan(/\p{Word}+/).count).to be <= 12 }
      end

      it "applies overlap" do
        no_overlap = described_class.chunks_by_size(doc, chars: 200, overlap: 0, at: :word)
        with_overlap = described_class.chunks_by_size(doc, chars: 200, overlap: 50, at: :word)
        # Overlap produces at least as many (usually more) windows.
        expect(with_overlap.size).to be >= no_overlap.size
      end
    end

    context "with statistics: true" do
      it "annotates each window with character_count and word_count" do
        windows = described_class.chunks_by_size(
          doc, chars: 300, at: :block, options: {statistics: true}
        )
        windows.each do |w|
          expect(w).to include(:character_count, :word_count)
          expect(w[:character_count]).to be_a(Integer)
          expect(w[:word_count]).to be_a(Integer)
        end
      end

      it "omits counts when statistics: false (default)" do
        windows = described_class.chunks_by_size(doc, chars: 300, at: :block)
        windows.each do |w|
          expect(w).not_to have_key(:character_count)
          expect(w).not_to have_key(:word_count)
        end
      end
    end

    context "filter-applied content" do
      it "emoji-expands before windowing" do
        src = ":rocket: " * 100  # 900 chars source → filtered ~200 chars (emoji 1 char each)
        windows = described_class.chunks_by_size(
          src, chars: 100, at: :block, options: {emoji_shortcodes: true}
        )
        windows.each { |w| expect(w[:content]).to include("🚀") }
        windows.each { |w| expect(w[:content]).not_to include(":rocket:") }
      end
    end

    context "edge cases" do
      it "returns empty array for nil source" do
        expect(described_class.chunks_by_size(nil, chars: 100)).to eq([])
      end

      it "returns empty array for empty source" do
        expect(described_class.chunks_by_size("", chars: 100)).to eq([])
      end
    end
  end

  describe "#chunks_by_size" do
    it "uses instance options" do
      md = "para one.\n\npara two.\n\npara three.\n"
      g = described_class.new(md, options: {emoji_shortcodes: true})
      windows = g.chunks_by_size(chars: 20, at: :block)
      expect(windows).not_to be_empty
    end

    it "returns empty array for an empty instance source" do
      expect(described_class.new("").chunks_by_size(chars: 100)).to eq([])
    end

    it "validates the same as the class method" do
      g = described_class.new("some text")
      expect { g.chunks_by_size }.to raise_error(ArgumentError, /at least one of/)
    end
  end
end
