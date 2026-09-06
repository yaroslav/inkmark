# frozen_string_literal: true

# Ractor is still marked experimental; silence the one-time warning so
# the suite output stays clean.
Warning[:experimental] = false

# Every example here runs the same call on the main Ractor and inside a
# freshly spawned Ractor and compares the two. The block handed to
# +in_ractor+ must not close over anything from the example (Ractor
# isolation forbids it); pass inputs as arguments instead.
RSpec.describe "Inkmark inside a Ractor" do
  def in_ractor(*args, &block)
    ractor = Ractor.new(*args, &block)
    ractor_result(ractor)
  rescue Ractor::RemoteError => e
    # Surface the worker's own exception so failures read naturally.
    raise e.cause
  end

  # Ractor#value arrived in Ruby 4.0; older Rubies use #take.
  def ractor_result(ractor)
    ractor.respond_to?(:value) ? ractor.value : ractor.take
  end

  # One document that touches every filter we want to see run in a worker:
  # front matter, emoji, autolink, a table, a highlighted code block.
  def doc
    <<~MD
      ---
      title: Hello
      tags: [a, b]
      ---

      # Heading :rocket:

      Visit https://example.com for **more**.

      | a | b |
      |---|---|
      | 1 | 2 |

      ```ruby
      def hi = puts "hi"
      ```

      ## Second
    MD
  end

  def rich_options
    {toc: true, statistics: true, frontmatter: true, extract: {links: true}}
  end

  it "calls into the native extension without Ractor::UnsafeError" do
    expect { in_ractor { Inkmark.to_html("canary") } }.not_to raise_error
  end

  describe "rendering" do
    it "renders HTML with the defaults" do
      expect(in_ractor(doc) { |src| Inkmark.to_html(src) }).to eq(Inkmark.to_html(doc))
    end

    it "renders HTML with a preset" do
      worker = in_ractor(doc) { |src| Inkmark.to_html(src, options: {preset: :recommended}) }
      expect(worker).to eq(Inkmark.to_html(doc, options: {preset: :recommended}))
    end

    it "parses front matter (YAML.safe_load runs in the worker)" do
      # Main first: on Ruby 3.3 a worker cannot `require`, so YAML has to
      # be loaded by the main Ractor before a worker parses front matter.
      main = Inkmark.new(doc, options: {frontmatter: true})
      expected = [main.frontmatter, main.to_html]
      worker = in_ractor(doc) do |src|
        md = Inkmark.new(src, options: {frontmatter: true})
        [md.frontmatter, md.to_html]
      end
      expect(worker).to eq(expected)
      expect(worker.first).to eq({"title" => "Hello", "tags" => ["a", "b"]})
    end

    it "highlights syntax and serves the theme CSS" do
      worker = in_ractor(doc) do |src|
        [Inkmark.to_html(src, options: {syntax_highlight: true}), Inkmark.highlight_css]
      end
      expect(worker).to eq([Inkmark.to_html(doc, options: {syntax_highlight: true}), Inkmark.highlight_css])
    end

    it "initializes the syntect caches from a worker in a fresh process" do
      # Everything else in this suite may already have warmed the
      # process-wide syntax set on the main Ractor; a subprocess makes the
      # first highlight happen inside a Ractor.
      script = <<~RUBY
        Warning[:experimental] = false
        require "inkmark"
        src = "```ruby\nx = 1\n```\n"
        r = Ractor.new(src) { |s| Inkmark.to_html(s, options: {syntax_highlight: true}) }
        worker = r.respond_to?(:value) ? r.value : r.take
        print(worker == Inkmark.to_html(src, options: {syntax_highlight: true}) ? "same" : "different")
      RUBY
      lib = File.expand_path("../lib", __dir__)
      output = IO.popen([RbConfig.ruby, "-I", lib, "-e", script], &:read)
      expect(output).to eq("same")
    end

    it "produces the other output formats and collected data" do
      worker = in_ractor(doc, rich_options) do |src, opts|
        md = Inkmark.new(src, options: opts)
        html = md.to_html
        [
          html, md.to_markdown, md.to_plain_text, md.toc.to_html, md.statistics, md.extracts,
          Inkmark.chunks_by_heading(src, options: opts),
          Inkmark.chunks_by_size(src, chars: 80, options: opts),
          Inkmark.truncate_markdown(src, words: 6, options: opts)
        ]
      end
      md = Inkmark.new(doc, options: rich_options)
      html = md.to_html
      expect(worker).to eq([
        html, md.to_markdown, md.to_plain_text, md.toc.to_html, md.statistics, md.extracts,
        Inkmark.chunks_by_heading(doc, options: rich_options),
        Inkmark.chunks_by_size(doc, chars: 80, options: rich_options),
        Inkmark.truncate_markdown(doc, words: 6, options: rich_options)
      ])
    end

    it "fires event handlers defined inside the worker" do
      worker = in_ractor(doc) do |src|
        seen = []
        html = Inkmark.new(src, options: {frontmatter: true}).on(:heading) { |h|
          seen << h.text
          h.level = 3
        }.to_html
        [seen, html]
      end
      seen = []
      html = Inkmark.new(doc, options: {frontmatter: true}).on(:heading) { |h|
        seen << h.text
        h.level = 3
      }.to_html
      expect(worker).to eq([seen, html])
      expect(worker.first).to eq(["Heading :rocket:", "Second"])
      expect(worker.last).to include("<h3>Heading :rocket:</h3>")
    end
  end

  describe "stress" do
    it "renders identically across 4 Ractors doing 100 renders each" do
      expected_html = Inkmark.to_html(doc, options: {preset: :recommended})
      expected_fm = Inkmark.new(doc, options: {frontmatter: true}).frontmatter

      ractors = Array.new(4) do
        Ractor.new(doc, expected_html, expected_fm) do |src, want_html, want_fm|
          100.times.count do
            md = Inkmark.new(src, options: {frontmatter: true})
            md.to_html
            Inkmark.to_html(src, options: {preset: :recommended}) == want_html && md.frontmatter == want_fm
          end
        end
      end
      expect(ractors.map { |r| ractor_result(r) }).to eq([100, 100, 100, 100])
    end
  end

  describe "Inkmark.default_options" do
    after { Inkmark.instance_variable_set(:@default_options, nil) }

    it "is readable from a worker" do
      result = in_ractor { [Inkmark.default_options.frozen?, Inkmark.default_options.tables] }
      expect(result).to eq([true, true])
    end

    it "reflects configuration done on the main Ractor" do
      Inkmark.configure { |o| o.math = true }
      expect(in_ractor { Inkmark.default_options.math }).to be true
    end

    it "seeds mutable per-instance options inside a worker" do
      result = in_ractor do
        md = Inkmark.new("x")
        md.options.tables = false
        [md.options.tables, md.options.frozen?]
      end
      expect(result).to eq([false, false])
    end
  end

  describe "Inkmark.highlight_themes" do
    it "is readable from a worker" do
      expect(in_ractor { Inkmark.highlight_themes }).to eq(Inkmark.highlight_themes)
    end
  end

  describe "Inkmark::Options" do
    it "reads and writes options through the generated accessors" do
      result = in_ractor do
        opts = Inkmark::Options.new
        opts.math = true
        [opts.math, opts.tables, opts.links]
      end
      expect(result).to eq([true, true, {autolink: false, nofollow: false, allowed_hosts: nil, allowed_schemes: nil}])
    end

    it "builds preset-based options" do
      result = in_ractor { Inkmark::Options.new(preset: :recommended, tables: false).to_h }
      expect(result).to eq(Inkmark::Options.new(preset: :recommended, tables: false).to_h)
    end

    it "builds the flat native hash" do
      result = in_ractor { Inkmark::Options.native_hash_from(preset: :commonmark, math: true) }
      expect(result).to eq(Inkmark::Options.native_hash_from(preset: :commonmark, math: true))
    end
  end
end
