# frozen_string_literal: true

require "spec_helper"

RSpec.describe "Inkmark event handlers" do
  describe "#on" do
    it "returns self for chaining" do
      md = Inkmark.new("text")
      expect(md.on(:paragraph) {}).to eq(md)
    end

    it "registers multiple handlers for the same kind" do
      md = Inkmark.new("# Hello")
      calls = []
      md.on(:heading) { calls << :first }
      md.on(:heading) { calls << :second }
      md.walk
      expect(calls).to eq([:first, :second])
    end
  end

  describe "#walk" do
    it "fires handlers without producing HTML" do
      md = Inkmark.new("# Hello\n\n## World")
      headings = []
      md.on(:heading) { |h| headings << {level: h.level, text: h.text} }
      result = md.walk
      expect(result).to eq(md)
      expect(headings).to eq([{level: 1, text: "Hello"}, {level: 2, text: "World"}])
    end

    it "returns self" do
      md = Inkmark.new("text")
      expect(md.walk).to eq(md)
    end

    it "returns self when source is empty" do
      md = Inkmark.new("")
      fired = false
      md.on(:paragraph) { fired = true }
      md.walk
      expect(fired).to be false
    end

    it "collects links" do
      md = Inkmark.new("See [this](https://example.net) and [that](https://other.com)")
      links = []
      md.on(:link) { |l| links << l.dest }
      md.walk
      expect(links).to eq(["https://example.net", "https://other.com"])
    end
  end

  describe "#to_html with handlers" do
    it "renders HTML normally when no mutations applied" do
      md = Inkmark.new("**bold**")
      fired = false
      md.on(:strong) { fired = true }
      html = md.to_html
      expect(fired).to be true
      expect(html).to eq("<p><strong>bold</strong></p>\n")
    end

    it "replaces element with html= override" do
      md = Inkmark.new("```mermaid\ngraph TD\n```")
      md.on(:code_block) do |c|
        c.html = "<div class=\"mermaid\">#{c.text}</div>" if c.lang == "mermaid"
      end
      html = md.to_html
      expect(html).to include('<div class="mermaid">')
      expect(html).not_to include("<pre>")
    end

    it "rewrites image dest" do
      md = Inkmark.new("![alt](http://origin.com/pic.png)")
      md.on(:image) { |img| img.dest = "https://cdn.example.com/#{File.basename(img.dest)}" }
      html = md.to_html
      expect(html).to include("cdn.example.com/pic.png")
      expect(html).not_to include("origin.com")
    end

    it "shifts heading level" do
      md = Inkmark.new("# Title\n\n## Section")
      md.on(:heading) { |h| h.level = [h.level + 1, 6].min }
      html = md.to_html
      expect(html).to include("<h2>")
      expect(html).to include("<h3>")
    end

    it "replaces paragraph matching a custom directive" do
      md = Inkmark.new("@available_since rails=3.8.0 core=2.0.8\n\nNormal paragraph.")
      md.on(:paragraph) do |p|
        if p.text =~ /\A@available_since\s+(.+)\z/
          attrs = $1.scan(/(\w+)=(\S+)/).map { |k, v| %( #{k}="#{v}") }.join
          p.html = "<AvailableSince#{attrs} />\n"
        end
      end
      html = md.to_html
      expect(html).to include("<AvailableSince")
      expect(html).to include('rails="3.8.0"')
      expect(html).to include("<p>Normal paragraph.</p>")
    end
  end

  describe "markdown= replacement" do
    it "replaces a paragraph with re-parsed markdown" do
      md = Inkmark.new("old paragraph")
      md.on(:paragraph) { |p| p.markdown = "**new** content" }
      html = md.to_html
      expect(html).to include("<strong>new</strong>")
      expect(html).not_to include("old paragraph")
    end

    it "allows block-level replacement (heading instead of paragraph)" do
      md = Inkmark.new("intro text")
      md.on(:paragraph) { |p| p.markdown = "# Promoted\n\nbody" }
      html = md.to_html
      expect(html).to include("<h1>Promoted</h1>")
      expect(html).to include("<p>body</p>")
      expect(html).not_to include("intro text")
    end

    it "html= takes priority over markdown=" do
      md = Inkmark.new("text")
      md.on(:paragraph) do |p|
        p.markdown = "**markdown**"
        p.html = "<custom>html</custom>"
      end
      html = md.to_html
      expect(html).to include("<custom>html</custom>")
      expect(html).not_to include("<strong>")
    end

    it "applies emoji filter to replacement markdown when enabled" do
      md = Inkmark.new("original", options: {emoji_shortcodes: true})
      md.on(:paragraph) { |p| p.markdown = ":rocket: launched" }
      html = md.to_html
      expect(html).to include("🚀")
    end

    it "has no effect during walk" do
      md = Inkmark.new("text")
      fired = false
      md.on(:paragraph) { |p|
        p.markdown = "replaced"
        fired = true
      }
      md.walk
      expect(fired).to be true
    end
  end

  describe "delete" do
    it "suppresses the element from output" do
      md = Inkmark.new("before\n\n![img](x.png)\n\nafter")
      md.on(:image) { |img| img.delete }
      html = md.to_html
      expect(html).not_to include("<img")
      expect(html).to include("before")
      expect(html).to include("after")
    end

    it "suppresses a heading" do
      md = Inkmark.new("# Secret\n\nBody text.")
      md.on(:heading) { |h| h.delete if h.text == "Secret" }
      html = md.to_html
      expect(html).not_to include("<h1>")
      expect(html).to include("Body text.")
    end
  end

  describe "event object fields" do
    it "exposes heading level and text" do
      md = Inkmark.new("## Hello World")
      events = []
      md.on(:heading) { |h| events << {level: h.level, text: h.text} }
      md.walk
      expect(events).to eq([{level: 2, text: "Hello World"}])
    end

    it "exposes heading id after headings: { ids: true }" do
      md = Inkmark.new("## Hello World", options: {headings: {ids: true}})
      events = []
      md.on(:heading) { |h| events << h.id }
      md.walk
      expect(events).to eq(["hello-world"])
    end

    it "exposes link dest and title" do
      md = Inkmark.new('[text](https://example.net "My title")')
      events = []
      md.on(:link) { |l| events << {dest: l.dest, title: l.title} }
      md.walk
      expect(events).to eq([{dest: "https://example.net", title: "My title"}])
    end

    it "exposes image dest and alt text" do
      md = Inkmark.new("![my alt](https://example.net/img.png)")
      events = []
      md.on(:image) { |i| events << {dest: i.dest, text: i.text} }
      md.walk
      expect(events).to eq([{dest: "https://example.net/img.png", text: "my alt"}])
    end

    it "exposes code_block lang and source" do
      md = Inkmark.new("```ruby\nputs 'hi'\n```")
      events = []
      md.on(:code_block) { |c| events << {lang: c.lang, text: c.text} }
      md.walk
      expect(events).to eq([{lang: "ruby", text: "puts 'hi'\n"}])
    end

    it "exposes inline code text" do
      md = Inkmark.new("Use `puts` to print.")
      events = []
      md.on(:code) { |c| events << c.text }
      md.walk
      expect(events).to eq(["puts"])
    end

    it "exposes depth" do
      md = Inkmark.new("> quoted")
      depths = {}
      md.on(:blockquote) { |b| depths[:blockquote] = b.depth }
      md.on(:paragraph) { |p| depths[:paragraph] = p.depth }
      md.walk
      expect(depths[:blockquote]).to eq(0)
      expect(depths[:paragraph]).to eq(1)
    end

    it "exposes parent_kind" do
      md = Inkmark.new("> quoted paragraph")
      parent = nil
      md.on(:paragraph) { |p| parent = p.parent_kind }
      md.walk
      expect(parent).to eq(:blockquote)
    end
  end

  describe "children access" do
    it "exposes table rows via children_of" do
      md = Inkmark.new("| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |")
      row_count = nil
      md.on(:table) { |t| row_count = t.children_of(:table_row).count }
      md.walk
      expect(row_count).to eq(2)
    end

    it "exposes table cell text" do
      md = Inkmark.new("| a | b |\n|---|---|\n| 1 | 2 |")
      cells = nil
      md.on(:table) do |t|
        rows = t.children_of(:table_row)
        cells = rows.first.children_of(:table_cell).map(&:text)
      end
      md.walk
      expect(cells).to eq(["1", "2"])
    end

    it "fires handlers post-order (children before parents)" do
      md = Inkmark.new("> quoted")
      order = []
      md.on(:blockquote) { order << :blockquote }
      md.on(:paragraph) { order << :paragraph }
      md.walk
      expect(order).to eq([:paragraph, :blockquote])
    end
  end

  describe "byte_range" do
    it "exposes byte range for headings" do
      source = "## Hello"
      md = Inkmark.new(source)
      range = nil
      md.on(:heading) { |h| range = h.byte_range }
      md.walk
      expect(range).to be_a(Range)
      expect(source[range]).to include("Hello")
    end

    it "exposes byte range for paragraphs" do
      source = "first\n\nsecond"
      md = Inkmark.new(source)
      ranges = []
      md.on(:paragraph) { |p| ranges << p.byte_range }
      md.walk
      expect(ranges.map { |r| source[r].strip }).to eq(["first", "second"])
    end

    it "exposes byte range for inline code" do
      source = "Use `puts` to print."
      md = Inkmark.new(source)
      range = nil
      md.on(:code) { |c| range = c.byte_range }
      md.walk
      expect(range).to be_a(Range)
      expect(source[range]).to include("puts")
    end

    it "exposes byte range for links (no autolink)" do
      source = "See [here](https://example.com)."
      md = Inkmark.new(source)
      range = nil
      md.on(:link) { |l| range = l.byte_range }
      md.walk
      expect(range).to be_a(Range)
      expect(source[range]).to include("here")
    end

    it "returns nil byte_range for links when autolink is enabled" do
      source = "See [here](https://example.net)."
      md = Inkmark.new(source, options: {links: {autolink: true}})
      range = :not_set
      md.on(:link) { |l| range = l.byte_range if l.dest == "https://example.net" }
      md.walk
      expect(range).to be_nil
    end
  end

  describe "ancestor_kinds" do
    it "includes parent kind in ancestor chain" do
      md = Inkmark.new("> quoted")
      ancestors = nil
      md.on(:paragraph) { |p| ancestors = p.ancestor_kinds }
      md.walk
      expect(ancestors).to include(:blockquote)
    end
  end

  describe "interaction with built-in filters" do
    it "sees emoji-resolved text in handlers (emoji runs before handlers)" do
      md = Inkmark.new("# :rocket: Launch", options: {emoji_shortcodes: true})
      text = nil
      md.on(:heading) { |h| text = h.text }
      md.walk
      expect(text).to include("🚀")
    end

    it "sees heading id set by headings: { ids: true }" do
      md = Inkmark.new("# Hello World", options: {headings: {ids: true}})
      id = nil
      md.on(:heading) { |h| id = h.id }
      md.walk
      expect(id).to eq("hello-world")
    end

    it "sees code_block as code_block even with syntax_highlight enabled" do
      md = Inkmark.new("```ruby\nputs 'hi'\n```", options: {syntax_highlight: true})
      seen_kind = nil
      md.on(:code_block) { |c| seen_kind = c.kind }
      md.walk
      expect(seen_kind).to eq(:code_block)
    end

    it "html= on code_block overrides syntax highlighting" do
      md = Inkmark.new("```ruby\nputs 'hi'\n```", options: {syntax_highlight: true})
      md.on(:code_block) { |c| c.html = "<custom>#{c.text}</custom>" }
      html = md.to_html
      expect(html).to include("<custom>")
      expect(html).not_to include("highlight")
    end
  end
end
