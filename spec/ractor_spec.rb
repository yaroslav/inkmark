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
