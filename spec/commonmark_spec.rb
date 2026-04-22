# frozen_string_literal: true

require "spec_helper"
require "json"
require "inkmark"
require_relative "support/commonmark_skiplist"

examples = JSON.parse(File.read(File.expand_path("fixtures/commonmark-spec.json", __dir__)))

RSpec.describe "CommonMark conformance" do
  # Inkmark disables GFM extensions here to compare apples-to-apples with the
  # CommonMark reference, and enables raw_html because the spec
  # expects raw HTML to pass through.
  base_opts = {
    gfm: false,
    tables: false,
    strikethrough: false,
    tasklists: false,
    footnotes: false,
    raw_html: true
  }

  examples.each do |ex|
    next if CommonMarkSkipList.skip?(ex["example"])

    describe "example ##{ex["example"]} (#{ex["section"]})" do
      it "renders according to the CommonMark spec" do
        actual = Inkmark.to_html(ex["markdown"], options: base_opts)
        expect(actual).to eq(ex["html"])
      end
    end
  end
end
