# frozen_string_literal: true

require "spec_helper"
require "inkmark"

fixture_dir = File.expand_path("fixtures/gfm", __dir__)

RSpec.describe "GFM extension rendering" do
  Dir["#{fixture_dir}/*.md"].sort.each do |md_path|
    html_path = md_path.sub(/\.md\z/, ".html")
    name = File.basename(md_path, ".md")

    describe name do
      it "renders to the expected HTML" do
        source = File.read(md_path)
        expected = File.read(html_path)
        expect(Inkmark.to_html(source)).to eq(expected)
      end
    end
  end
end
