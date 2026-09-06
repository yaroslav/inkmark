# frozen_string_literal: true

require "spec_helper"
require "inkmark/options"

RSpec.describe Inkmark::Options do
  describe ".new" do
    context "without arguments" do
      it "starts with GFM defaults on" do
        opts = described_class.new
        expect(opts[:gfm]).to be true
        expect(opts[:tables]).to be true
        expect(opts[:strikethrough]).to be true
        expect(opts[:tasklists]).to be true
        expect(opts[:footnotes]).to be true
      end

      it "has raw HTML rendering off" do
        expect(described_class.new[:raw_html]).to be false
      end

      it "has all non-GFM extensions off" do
        opts = described_class.new
        expect(opts[:smart_punctuation]).to be false
        expect(opts[:math]).to be false
        expect(opts[:wikilinks]).to be false
      end
    end

    context "with an overrides hash" do
      it "applies the overrides on top of the defaults" do
        opts = described_class.new(tables: false, math: true)
        expect(opts[:tables]).to be false
        expect(opts[:math]).to be true
        expect(opts[:gfm]).to be true  # untouched
      end
    end

    context "with an unknown option key" do
      it "raises ArgumentError naming the key" do
        expect { described_class.new(taples: true) }
          .to raise_error(ArgumentError, /unknown Inkmark option: :taples/)
      end
    end
  end

  describe "#[]=" do
    context "with a known key" do
      it "updates the value" do
        opts = described_class.new
        opts[:tables] = false
        expect(opts[:tables]).to be false
      end
    end

    context "with an unknown key" do
      it "raises ArgumentError" do
        opts = described_class.new
        expect { opts[:xyzzy] = true }
          .to raise_error(ArgumentError, /unknown Inkmark option: :xyzzy/)
      end
    end
  end

  describe "method accessors" do
    it "reads the value" do
      expect(described_class.new.tables).to be true
    end

    it "writes the value" do
      opts = described_class.new
      opts.tables = false
      expect(opts.tables).to be false
      expect(opts[:tables]).to be false
    end

    it "stays in sync with []= writes" do
      opts = described_class.new
      opts[:tables] = false
      expect(opts.tables).to be false
    end
  end

  describe "#dup" do
    it "returns a new instance with the same values" do
      a = described_class.new(tables: false)
      b = a.dup
      expect(b).not_to equal(a)
      expect(b[:tables]).to be false
    end

    it "is not affected by later mutations of the original" do
      a = described_class.new
      b = a.dup
      a[:tables] = false
      expect(b[:tables]).to be true
    end

    it "does not aliasing-bleed when the copy is mutated" do
      a = described_class.new
      b = a.dup
      b[:tables] = false
      expect(a[:tables]).to be true
    end
  end

  describe "#to_h" do
    it "returns a hash of all current values" do
      h = described_class.new.to_h
      expect(h).to be_a(Hash)
      expect(h[:gfm]).to be true
      expect(h.size).to eq(Inkmark::Options::DEFAULTS.size)
    end

    it "returns a copy that does not bleed into the instance" do
      opts = described_class.new
      h = opts.to_h
      h[:tables] = false
      expect(opts[:tables]).to be true
    end
  end

  describe "#merge" do
    it "returns a new instance with the overrides applied" do
      a = described_class.new
      b = a.merge(tables: false)
      expect(b).not_to equal(a)
      expect(a[:tables]).to be true
      expect(b[:tables]).to be false
    end
  end

  describe "#==" do
    it "is true for two instances with identical values" do
      expect(described_class.new).to eq(described_class.new)
    end

    it "is false when values differ" do
      expect(described_class.new).not_to eq(described_class.new(tables: false))
    end

    it "is false when compared with a non-Options object" do
      expect(described_class.new).not_to eq({})
    end
  end

  describe "nested element-policy hashes" do
    describe ":headings" do
      it "defaults to all sub-keys off" do
        expect(described_class.new[:headings]).to eq(attributes: false, ids: false)
      end

      it "deep-merges a partial override over defaults" do
        opts = described_class.new(headings: {ids: true})
        expect(opts[:headings]).to eq(attributes: false, ids: true)
      end

      it "deep-merges on setter writes" do
        opts = described_class.new(headings: {ids: true})
        opts[:headings] = {attributes: true}
        expect(opts[:headings]).to eq(attributes: true, ids: true)
      end

      it "exposes a hash accessor" do
        opts = described_class.new(headings: {ids: true})
        expect(opts.headings).to eq(attributes: false, ids: true)
      end

      it "accepts writes via the hash accessor" do
        opts = described_class.new
        opts.headings = {attributes: true}
        expect(opts[:headings]).to eq(attributes: true, ids: false)
      end

      it "raises on unknown sub-keys" do
        expect { described_class.new(headings: {bogus: true}) }
          .to raise_error(ArgumentError, /unknown headings key/)
      end

      it "raises on wrong sub-value types" do
        expect { described_class.new(headings: {ids: "yes"}) }
          .to raise_error(ArgumentError, /invalid value for headings\[:ids\]/)
      end
    end

    describe ":images" do
      it "defaults to all sub-keys off / nil" do
        expect(described_class.new[:images])
          .to eq(lazy: false, allowed_hosts: nil, allowed_schemes: nil)
      end

      it "deep-merges a partial override" do
        opts = described_class.new(images: {allowed_hosts: ["*.cdn.com"]})
        expect(opts[:images]).to eq(
          lazy: false, allowed_hosts: ["*.cdn.com"], allowed_schemes: nil
        )
      end

      it "raises on unknown sub-keys" do
        expect { described_class.new(images: {whatever: true}) }
          .to raise_error(ArgumentError, /unknown images key/)
      end
    end

    describe ":links" do
      it "defaults to all sub-keys off / nil" do
        expect(described_class.new[:links]).to eq(
          autolink: false, nofollow: false,
          allowed_hosts: nil, allowed_schemes: nil
        )
      end

      it "deep-merges a partial override" do
        opts = described_class.new(links: {nofollow: true, allowed_schemes: ["https"]})
        expect(opts[:links]).to eq(
          autolink: false, nofollow: true,
          allowed_hosts: nil, allowed_schemes: ["https"]
        )
      end

      it "raises on wrong sub-value types" do
        expect { described_class.new(links: {allowed_hosts: "not-an-array"}) }
          .to raise_error(ArgumentError, /invalid value for links\[:allowed_hosts\]/)
      end
    end

    describe "#to_h" do
      it "returns nested hashes matching the input shape" do
        h = described_class.new(images: {lazy: true}).to_h
        expect(h[:images]).to eq(lazy: true, allowed_hosts: nil, allowed_schemes: nil)
      end

      it "deep-dups nested hashes so the instance is not mutated" do
        opts = described_class.new
        h = opts.to_h
        h[:images][:lazy] = true
        expect(opts[:images][:lazy]).to be false
      end
    end

    describe "#merge" do
      it "deep-merges nested hashes from the other options" do
        a = described_class.new(images: {lazy: true})
        b = a.merge(images: {allowed_schemes: ["https"]})
        expect(b[:images]).to eq(
          lazy: true, allowed_hosts: nil, allowed_schemes: ["https"]
        )
        expect(a[:images]).to eq(
          lazy: true, allowed_hosts: nil, allowed_schemes: nil
        )
      end
    end

    describe "#dup" do
      it "deep-copies nested hashes" do
        a = described_class.new(images: {lazy: true})
        b = a.dup
        b[:images] = {allowed_hosts: ["*.cdn.com"]}
        expect(a[:images][:allowed_hosts]).to be_nil
      end
    end
  end

  describe "presets" do
    it "applies :gfm by default (matches DEFAULTS)" do
      opts = described_class.new
      expect(opts[:gfm]).to be true
      expect(opts[:tables]).to be true
      expect(opts[:strikethrough]).to be true
      expect(opts[:tasklists]).to be true
      expect(opts[:footnotes]).to be true
      expect(opts[:gfm_tag_filter]).to be true
      expect(opts[:smart_punctuation]).to be false
      expect(opts[:syntax_highlight]).to be false
    end

    it "applies :commonmark (GFM off)" do
      opts = described_class.new(preset: :commonmark)
      expect(opts[:gfm]).to be false
      expect(opts[:gfm_tag_filter]).to be false
      expect(opts[:tables]).to be false
      expect(opts[:strikethrough]).to be false
      expect(opts[:tasklists]).to be false
      expect(opts[:footnotes]).to be false
    end

    it "applies :recommended (full-featured)" do
      opts = described_class.new(preset: :recommended)
      expect(opts[:gfm]).to be true
      expect(opts[:smart_punctuation]).to be true
      expect(opts[:emoji_shortcodes]).to be true
      expect(opts[:syntax_highlight]).to be true
      expect(opts[:hard_wrap]).to be true
      expect(opts[:frontmatter]).to be true
      expect(opts[:raw_html]).to be false
      expect(opts[:headings]).to eq(attributes: false, ids: true)
      expect(opts[:images]).to eq(
        lazy: true, allowed_hosts: nil, allowed_schemes: ["http", "https"]
      )
      expect(opts[:links]).to eq(
        autolink: true, nofollow: true,
        allowed_hosts: nil, allowed_schemes: ["http", "https", "mailto"]
      )
    end

    it "applies :trusted (:recommended + raw HTML)" do
      opts = described_class.new(preset: :trusted)
      expect(opts[:raw_html]).to be true
      expect(opts[:gfm_tag_filter]).to be true
      expect(opts[:smart_punctuation]).to be true
      expect(opts[:syntax_highlight]).to be true
      expect(opts[:links][:nofollow]).to be true
    end

    it "lets subsequent options override preset values (top-level)" do
      opts = described_class.new(preset: :recommended, smart_punctuation: false)
      expect(opts[:smart_punctuation]).to be false
      expect(opts[:syntax_highlight]).to be true
    end

    it "deep-merges override nested hashes over preset nested hashes" do
      opts = described_class.new(
        preset: :recommended,
        links: {autolink: false}
      )
      expect(opts[:links]).to eq(
        autolink: false, nofollow: true,
        allowed_hosts: nil, allowed_schemes: ["http", "https", "mailto"]
      )
    end

    it "does not surface :preset in to_h" do
      opts = described_class.new(preset: :recommended)
      expect(opts.to_h).not_to have_key(:preset)
    end

    it "does not surface :preset in to_native_hash" do
      opts = described_class.new(preset: :recommended)
      expect(opts.to_native_hash).not_to have_key(:preset)
    end

    it "raises on unknown preset name" do
      expect { described_class.new(preset: :bogus) }
        .to raise_error(ArgumentError, /unknown preset: :bogus/)
    end

    it "re-applies a preset via #merge without disturbing the receiver" do
      a = described_class.new
      b = a.merge(preset: :recommended)
      expect(a[:smart_punctuation]).to be false
      expect(b[:smart_punctuation]).to be true
    end

    it "#merge without :preset preserves the receiver's state" do
      a = described_class.new(preset: :recommended)
      b = a.merge(smart_punctuation: false)
      expect(b[:smart_punctuation]).to be false
      expect(b[:syntax_highlight]).to be true  # kept from original
    end
  end

  describe "PRESETS_NATIVE_HASH" do
    it "has one entry per preset" do
      expect(described_class::PRESETS_NATIVE_HASH.keys)
        .to match_array(described_class::PRESETS.keys)
    end

    it "matches Options.new(preset: name).to_native_hash_frozen" do
      described_class::PRESETS.each_key do |name|
        expected = described_class.new(preset: name).to_native_hash_frozen
        expect(described_class::PRESETS_NATIVE_HASH[name]).to eq(expected)
      end
    end

    it "returns frozen hashes" do
      described_class::PRESETS_NATIVE_HASH.each_value do |h|
        expect(h).to be_frozen
      end
    end
  end

  describe ".native_hash_from" do
    it "returns the cached frozen hash for empty input" do
      expect(described_class.native_hash_from({}))
        .to equal(described_class::PRESETS_NATIVE_HASH[described_class::DEFAULT_PRESET])
    end

    it "returns the cached frozen hash for preset-only input" do
      expect(described_class.native_hash_from(preset: :recommended))
        .to equal(described_class::PRESETS_NATIVE_HASH[:recommended])
    end

    it "applies top-level overrides on top of the preset's cached hash" do
      h = described_class.native_hash_from(tables: false, strikethrough: false)
      expect(h[:tables]).to be false
      expect(h[:strikethrough]).to be false
      expect(h[:gfm]).to be true  # from :gfm preset
    end

    it "flattens nested :headings overrides into heading_ids / heading_attributes" do
      h = described_class.native_hash_from(headings: {ids: true, attributes: true})
      expect(h[:heading_ids]).to be true
      expect(h[:heading_attributes]).to be true
    end

    it "flattens nested :images overrides" do
      h = described_class.native_hash_from(images: {lazy: true, allowed_hosts: ["*.cdn.com"]})
      expect(h[:lazy_images]).to be true
      expect(h[:allowed_image_hosts]).to eq(["*.cdn.com"])
    end

    it "flattens nested :links overrides (all sub-keys)" do
      h = described_class.native_hash_from(
        links: {autolink: true, nofollow: true, allowed_schemes: ["https"]}
      )
      expect(h[:autolink]).to be true
      expect(h[:nofollow_external_links]).to be true
      expect(h[:allowed_link_schemes]).to eq(["https"])
    end

    it "deep-merges nested overrides over the preset's flat values" do
      # :recommended has images: { lazy: true, allowed_schemes: ["http","https"] }
      h = described_class.native_hash_from(preset: :recommended, images: {lazy: false})
      expect(h[:lazy_images]).to be false
      expect(h[:allowed_image_schemes]).to eq(["http", "https"])  # kept from preset
    end

    it "expands toc: {depth: N} to toc + toc_depth" do
      h = described_class.native_hash_from(toc: {depth: 3})
      expect(h[:toc]).to be true
      expect(h[:toc_depth]).to eq(3)
    end

    it "omits toc_depth when toc is a plain boolean" do
      h = described_class.native_hash_from(toc: true)
      expect(h[:toc]).to be true
      expect(h).not_to have_key(:toc_depth)
    end

    it "returns frozen hashes" do
      expect(described_class.native_hash_from(tables: false)).to be_frozen
    end

    it "raises on unknown top-level key" do
      expect { described_class.native_hash_from(taples: true) }
        .to raise_error(ArgumentError, /unknown Inkmark option: :taples/)
    end

    it "raises on wrong value type" do
      expect { described_class.native_hash_from(tables: "yes") }
        .to raise_error(ArgumentError, /invalid value for tables/)
    end

    it "raises on unknown nested sub-key" do
      expect { described_class.native_hash_from(headings: {bogus: true}) }
        .to raise_error(ArgumentError, /unknown headings key/)
    end

    it "raises on toc depth out of 1..6" do
      expect { described_class.native_hash_from(toc: {depth: 9}) }
        .to raise_error(ArgumentError, /invalid value for toc depth/)
    end

    it "raises on unknown preset name" do
      expect { described_class.native_hash_from(preset: :bogus) }
        .to raise_error(ArgumentError, /unknown preset/)
    end

    it "matches Options.new(overrides).to_native_hash_frozen" do
      [
        {tables: false},
        {preset: :recommended, smart_punctuation: false},
        {images: {lazy: true}, links: {nofollow: true}},
        {toc: {depth: 2}, headings: {ids: true}},
        {preset: :trusted}
      ].each do |overrides|
        expected = described_class.new(overrides).to_native_hash_frozen
        got = described_class.native_hash_from(overrides)
        expect(got).to eq(expected), "mismatch for #{overrides.inspect}"
      end
    end
  end

  describe "#to_native_hash" do
    it "flattens nested hashes into Rust-facing flat keys" do
      opts = described_class.new(
        headings: {ids: true, attributes: true},
        images: {lazy: true, allowed_hosts: ["*.cdn.com"]},
        links: {nofollow: true, allowed_schemes: ["https"]}
      )
      h = opts.to_native_hash
      expect(h).to include(
        heading_ids: true,
        heading_attributes: true,
        lazy_images: true,
        allowed_image_hosts: ["*.cdn.com"],
        allowed_image_schemes: nil,
        nofollow_external_links: true,
        allowed_link_schemes: ["https"],
        allowed_link_hosts: nil,
        autolink: false
      )
    end

    it "is a fresh mutable hash each call" do
      opts = described_class.new
      a = opts.to_native_hash
      b = opts.to_native_hash
      expect(a).not_to equal(b)
      a[:foo] = :bar
      expect(b).not_to have_key(:foo)
    end
  end

  describe "Ractor shareability" do
    %i[
      HEADINGS_SCHEMA IMAGES_SCHEMA LINKS_SCHEMA NESTED_SCHEMAS NESTED_TO_FLAT
      DEFAULTS TYPES EXTRACT_KINDS PRESETS PRESETS_NATIVE_HASH
    ].each do |name|
      it "#{name} is deeply frozen" do
        expect(Ractor.shareable?(described_class.const_get(name))).to be true
      end
    end

    it "#to_native_hash_frozen is deeply frozen" do
      opts = described_class.new(
        images: {allowed_hosts: ["*.cdn.com"]},
        extract: {images: true}
      )
      expect(Ractor.shareable?(opts.to_native_hash_frozen)).to be true
    end

    it "#to_native_hash_frozen leaves the caller's arrays mutable" do
      hosts = ["*.cdn.com"]
      opts = described_class.new(images: {allowed_hosts: hosts})
      opts.to_native_hash_frozen
      expect(hosts).not_to be_frozen
    end
  end
end
