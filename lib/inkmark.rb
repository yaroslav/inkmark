# frozen_string_literal: true

# Inkmark is a very fast, feature-rich, AI-first CommonMark/GFM
# markdown renderer backed by the Rust pulldown-cmark parser.
#
# Default behavior: GFM extensions (tables, strikethrough, tasklists,
# footnotes) are enabled; raw HTML is suppressed. Override via options.
#
# ### Presets
#
# Four named bundles of options cover the common profiles:
#
# - +:gfm+ (the default): CommonMark + core GFM only.
# - +:commonmark+: strict CommonMark, no GFM.
# - +:recommended+: opinionated bundle for modern web content (smart
#   punctuation, auto heading IDs, lazy images, autolinks + nofollow,
#   URL scheme allowlists, emoji shortcodes, syntax highlighting,
#   frontmatter).
# - +:trusted+: +:recommended+ plus raw-HTML pass-through. **Use only
#   for fully trusted content.**
#
# See {Inkmark::Options::PRESETS}.
#
# ### Raw HTML safety
#
# Raw HTML is suppressed by default; every +<tag>+ in the source is
# escaped to text. Enable pass-through with +raw_html: true+ or the
# +:trusted+ preset **only for trusted input**. Inkmark does not
# sanitize raw HTML beyond the narrow GFM tagfilter; sanitize before rendering
# user-influenced content.
#
# @example Class-method shortcut
#   Inkmark.to_html("**hello**")
#   #=> "<p><strong>hello</strong></p>\n"
#
# @example Instance form with options
#   g = Inkmark.new("# hi", options: { tables: false })
#   g.to_html
#
# @example Mutable options after construction
#   g = Inkmark.new("# hi")
#   g.options.tables = false
#   g.to_html
#
# @example Recommended profile
#   Inkmark.to_html(md, options: { preset: :recommended })
class Inkmark
  # Base error class for Inkmark-specific runtime failures.
  class Error < StandardError; end
end

require_relative "inkmark/version"
require_relative "inkmark/options"
require_relative "inkmark/event"
require_relative "inkmark/toc"
require_relative "inkmark/native"

class Inkmark
  class << self
    # Render +source+ markdown to HTML in one call.
    #
    # This is a class-method fast path that skips Inkmark instance and
    # Options copy allocation for the common one-shot render pattern.
    # When the caller passes +options: nil+ (the default), we reuse the
    # frozen hash memoized on {default_options} by
    # {Inkmark::Options#to_native_hash_frozen}. {configure} and
    # {default_options=} install a whole new frozen instance rather than
    # mutating the shared one, so the next render sees the new values
    # without any stale-cache bugs.
    #
    # **Raw HTML safety.** +raw_html: false+ (the default) escapes
    # every raw HTML tag in the source—safe for untrusted input.
    # Enable +raw_html: true+ (or +preset: :trusted+) only for
    # content you fully trust, and run the output through a dedicated
    # HTML sanitizer before displaying it.
    #
    # @param source [String, nil] the markdown source to render
    # @param options [Hash, Inkmark::Options, nil] rendering options; merged
    #   over {default_options} when a Hash is supplied. Accepts
    #   +preset: :name+ (see {Inkmark::Options::PRESETS}).
    # @return [String] the rendered HTML
    # @raise [TypeError] if +options+ is not a Hash, Inkmark::Options, or nil
    # @example
    #   Inkmark.to_html("**bold**")  #=> "<p><strong>bold</strong></p>\n"
    # @example With a preset
    #   Inkmark.to_html(md, options: { preset: :recommended })
    def to_html(source, options: nil)
      source = source.to_s
      return "" if source.empty?
      _native_to_html(source, resolve_frozen_options(options))
    end

    # Render +source+ markdown through the filter pipeline and serialize back
    # to Markdown text.
    #
    # The same event-level filters as {to_html} are applied (emoji expansion,
    # allowlists, autolink, etc.), then the event stream is serialized back to
    # Markdown using pulldown-cmark-to-cmark. Use this as a preprocessing step
    # in pipelines that consume Markdown: LLM prompts, secondary renderers,
    # content storage.
    #
    # HTML-emitting filters (+syntax_highlight+, +images: { lazy: true }+,
    # +links: { nofollow: true }+) embed raw HTML verbatim in the
    # Markdown output when enabled. That is valid CommonMark but may
    # break downstream consumers.
    # See the "Markdown-to-Markdown pipeline" section in the README.
    #
    # @param source [String, nil] the markdown source to process
    # @param options [Hash, Inkmark::Options, nil] rendering options
    # @return [String] the filtered Markdown
    def to_markdown(source, options: nil)
      source = source.to_s
      return "" if source.empty?
      _native_to_markdown(source, resolve_frozen_options(options))
    end

    # Chunk +source+ by heading into an Array of section Hashes. Each
    # section's +:content+ is filter-applied Markdown (emoji expanded,
    # autolinks resolved, allowlists applied). Designed for feeding
    # RAG / embedding pipelines that want pre-HTML chunks with clean
    # content.
    #
    # Sections are hierarchical: a +##+ section's +:content+ includes
    # any nested +###+ subsections, which also appear as their own
    # entries. Content before the first heading (if any) is emitted
    # as a preamble entry with +heading: nil+ and +level: 0+.
    #
    # Filter the returned array with plain +Enumerable+—by heading,
    # level, id, or any other field. See the "Section extraction" in
    # the README for recipes.
    #
    # **HTML-emitting filters** (+syntax_highlight+, +images: { lazy: true }+,
    # +links: { nofollow: true }+) embed raw HTML into +:content+ when
    # enabled. For RAG pipelines you almost always want these off so
    # chunks stay pure Markdown.
    #
    # @param source [String, nil] the markdown source
    # @param options [Hash, Inkmark::Options, nil] rendering options
    # @return [Array<Hash>] section records
    # @example Fetch one section
    #   Inkmark.chunks_by_heading(readme).find { |s| s[:heading] == "Installation" }
    # @example Filter by heading pattern
    #   Inkmark.chunks_by_heading(readme).select { |s| s[:heading]&.match?(/install/i) }
    # @example RAG chunking
    #   Inkmark.chunks_by_heading(readme).each do |s|
    #     embed_and_store("#{s[:heading]}\n\n#{s[:content]}") if s[:heading]
    #   end
    def chunks_by_heading(source, options: nil, truncate: nil)
      source = source.to_s
      return [] if source.empty?

      opts_hash = resolve_mutable_options(options)
      opts_hash[:truncate] = normalize_truncate_params(truncate) if truncate
      _native_chunks_by_heading(source, opts_hash)
    end

    # Split +source+ into sliding-window chunks bounded by a character
    # and/or word budget. Adjacent chunks can share trailing context
    # via +overlap+, which preserves continuity for embedding models.
    # Unlike {chunks_by_heading}, this ignores document structure and
    # walks the filter-applied Markdown sequentially — useful for
    # heading-free or heading-uneven documents.
    #
    # @param source [String, nil] the markdown source
    # @param chars [Integer, nil] max characters per chunk
    # @param words [Integer, nil] max Unicode words per chunk; at
    #   least one of +chars+/+words+ must be set
    # @param overlap [Integer] chars carried from the end of the
    #   previous chunk into the start of the next. Defaults to 0.
    #   Must be less than +chars+ when +chars+ is set.
    # @param at [Symbol] +:block+ (valid-Markdown cut, oversized
    #   blocks emit as their own chunk) or +:word+ (word-boundary
    #   cut, may split open constructs).
    # @param options [Hash, Inkmark::Options, nil] rendering options
    # @return [Array<Hash>] each +{index:, content:}+, plus
    #   +:character_count+/+:word_count+ when +statistics: true+
    # @raise [ArgumentError] on invalid parameter combinations
    # @example
    #   Inkmark.chunks_by_size(readme, chars: 500, overlap: 50)
    def chunks_by_size(source, chars: nil, words: nil, overlap: 0, at: :block, options: nil)
      source = source.to_s
      return [] if source.empty?

      opts_hash = resolve_mutable_options(options)
      opts_hash[:__window] = normalize_window_params(
        chars: chars, words: words, overlap: overlap, at: at
      )
      _native_chunks_by_size(source, opts_hash)
    end

    # Truncate a Markdown document to fit a char and/or word budget.
    # Returns filter-applied Markdown cut at either the last block
    # boundary that fits (+at: :block+) or the last Unicode word
    # boundary that fits (+at: :word+).
    #
    # Designed as a preprocessing step for LLM context-window budgeting
    # and RAG chunk normalization. The marker (default +"…"+) is
    # appended only when truncation actually occurred and counts toward
    # the budget, so +chars: 4000+ always yields output ≤ 4000
    # codepoints.
    #
    # @param source [String, nil] the markdown source
    # @param chars [Integer, nil] maximum codepoint count; at least
    #   one of +chars+/+words+ must be set
    # @param words [Integer, nil] maximum Unicode word count
    # @param at [Symbol] +:block+ (valid-Markdown cut) or +:word+
    #   (word-boundary cut; may split open constructs)
    # @param marker [String, nil] appended when truncation occurs.
    #   Pass +nil+ to suppress. Defaults to +"…"+ (U+2026).
    # @param options [Hash, Inkmark::Options, nil] rendering options
    # @return [String] truncated Markdown, or the source unchanged
    #   when it already fits
    # @raise [ArgumentError] if neither chars nor words is set,
    #   +at+ is not +:block+/+:word+, or the marker exceeds the budget
    def truncate_markdown(source, chars: nil, words: nil, at: :block, marker: "…", options: nil)
      source = source.to_s
      return "" if source.empty?

      params = normalize_truncate_params(
        chars: chars, words: words, at: at, marker: marker
      )
      # truncate's native binding requires an options Hash; unlike the
      # to_html/to_plain_text bindings it has no nil fast path, so fall
      # back to the default options hash when the resolver returns nil.
      _native_truncate_markdown(
        source, params,
        resolve_frozen_options(options) || default_options.to_native_hash_frozen
      )
    end

    # Render +source+ through the filter pipeline and serialize to plain
    # text. Markdown syntax (emphasis, headings, list bullets, fences)
    # is stripped; inline content is preserved. Links become
    # +"text (url)"+; images become +"alt (src)"+; tables are
    # tab-separated; code blocks keep their raw body.
    #
    # Designed as a preprocessor for embedding models, token counting,
    # LLM input, and any downstream consumer that treats Markdown
    # syntax as noise.
    #
    # @param source [String, nil] the markdown source
    # @param options [Hash, Inkmark::Options, nil] rendering options
    # @return [String] plain-text output
    def to_plain_text(source, options: nil)
      source = source.to_s
      return "" if source.empty?
      _native_to_plain_text(source, resolve_frozen_options(options))
    end

    # Normalize and validate truncation params coming from either the
    # {.truncate_markdown} kwargs or the {.chunks_by_heading}
    # +truncate:+ kwarg. Accepts a Hash with +:chars+/+:words+/+:at+/
    # +:marker+ keys, or positional kwargs (collected by the caller
    # into a Hash). Returns a Hash ready to hand to the native side.
    #
    # @api private
    def normalize_truncate_params(params)
      if params.respond_to?(:to_hash)
        params = params.to_hash
      end
      unless params.is_a?(Hash)
        raise TypeError, "truncate must be a Hash, got #{params.class}"
      end

      unknown = params.keys - [:chars, :words, :at, :marker]
      unless unknown.empty?
        raise ArgumentError, "unknown truncate key(s): #{unknown.inspect}; " \
          "expected :chars, :words, :at, :marker"
      end

      chars = params[:chars]
      words = params[:words]
      at = params.fetch(:at, :block)
      marker = params.fetch(:marker, "…")

      if chars.nil? && words.nil?
        raise ArgumentError, "truncate requires at least one of :chars or :words"
      end
      if chars && !chars.is_a?(Integer)
        raise ArgumentError, ":chars must be an Integer, got #{chars.class}"
      end
      if words && !words.is_a?(Integer)
        raise ArgumentError, ":words must be an Integer, got #{words.class}"
      end
      unless %i[block word].include?(at)
        raise ArgumentError, ":at must be :block or :word, got #{at.inspect}"
      end
      unless marker.nil? || marker.is_a?(String)
        raise ArgumentError, ":marker must be a String or nil, got #{marker.class}"
      end
      if marker && chars && marker.length >= chars
        raise ArgumentError, ":marker (#{marker.length} chars) must be shorter than :chars budget (#{chars})"
      end

      {chars: chars, words: words, at: at.to_s, marker: marker}
    end

    # Validate sliding-window chunking params. Keeps {.chunks_by_size}
    # tight by raising on obvious misconfiguration rather than silent
    # clamping — invalid overlap or missing budget is almost always a
    # swapped-arg bug.
    #
    # @api private
    def normalize_window_params(chars:, words:, overlap:, at:)
      if chars.nil? && words.nil?
        raise ArgumentError, "chunks_by_size requires at least one of :chars or :words"
      end
      if chars && !chars.is_a?(Integer)
        raise ArgumentError, ":chars must be an Integer, got #{chars.class}"
      end
      if words && !words.is_a?(Integer)
        raise ArgumentError, ":words must be an Integer, got #{words.class}"
      end
      if chars && chars <= 0
        raise ArgumentError, ":chars must be positive, got #{chars}"
      end
      if words && words <= 0
        raise ArgumentError, ":words must be positive, got #{words}"
      end
      unless overlap.is_a?(Integer)
        raise ArgumentError, ":overlap must be an Integer, got #{overlap.class}"
      end
      if overlap < 0
        raise ArgumentError, ":overlap must be non-negative, got #{overlap}"
      end
      if chars && overlap >= chars
        raise ArgumentError, ":overlap (#{overlap}) must be less than :chars budget (#{chars})"
      end
      unless %i[block word].include?(at)
        raise ArgumentError, ":at must be :block or :word, got #{at.inspect}"
      end

      {chars: chars, words: words, overlap: overlap, at: at.to_s}
    end

    # Return the CSS stylesheet for syntax-highlighted code blocks.
    # Pair this with +syntax_highlight: true+ in the rendering options.
    #
    # @param theme [String, nil] syntect theme name; defaults to
    #   "base16-ocean.dark". Call {highlight_themes} for available names.
    # @return [String] CSS text suitable for a +<style>+ tag or +.css+ file
    # @raise [ArgumentError] if the theme name is not recognized
    # @example
    #   Inkmark.highlight_css
    #   Inkmark.highlight_css(theme: "InspiredGitHub")
    def highlight_css(theme: nil)
      _syntax_css(theme)
    end

    # Return an array of available syntax-highlighting theme names.
    # Memoized—the theme list is fixed at compile time. The memo is
    # warmed while this file loads (see the bottom of the file), so
    # non-main Ractors only ever read it.
    #
    # @return [Array<String>] frozen, with frozen elements
    def highlight_themes
      @highlight_themes ||= _syntax_themes.each(&:freeze).freeze
    end

    # The process-wide default options, used when a render is given no
    # options of its own. The instance is frozen all the way down and
    # shared by every thread and Ractor in the process; change it with
    # {configure} or {default_options=}, never in place.
    #
    # @return [Inkmark::Options] frozen
    def default_options
      @default_options || DEFAULT_OPTIONS
    end

    # Replace the process-wide default options. The value is copied and
    # frozen all the way down (+Ractor.make_shareable+), so the caller
    # keeps its own object mutable and worker Ractors can read the
    # result. Call this on the main Ractor before spawning workers.
    #
    # @param value [Hash, Inkmark::Options] new defaults; a Hash is
    #   converted to Inkmark::Options
    # @return [Inkmark::Options] the stored, frozen options
    # @raise [TypeError] if +value+ is not a Hash or Inkmark::Options
    # @example
    #   Inkmark.default_options = { preset: :recommended, math: true }
    def default_options=(value)
      options =
        case value
        when Inkmark::Options then value
        when Hash then Inkmark::Options.new(value)
        else raise TypeError, "default_options must be a Hash or Inkmark::Options, got #{value.class}"
        end
      # Copy-on-write of a shareable value: main-Ractor configuration.
      @default_options = Ractor.make_shareable(options, copy: true) # audition:disable class-level-state
    end

    # Adjust the process-wide default options. Yields a mutable copy of
    # the current {default_options}; the result is stored frozen through
    # {default_options=}. Successive calls build on each other. Call this
    # on the main Ractor before spawning workers.
    #
    # @yieldparam options [Inkmark::Options] a mutable copy of the
    #   current defaults
    # @return [Inkmark::Options] the stored, frozen options
    # @example In an application initializer
    #   Inkmark.configure do |options|
    #     options.math = true
    #     options.links = { nofollow: true }
    #   end
    def configure
      options = default_options.dup
      yield options
      self.default_options = options
      default_options
    end

    private

    # Resolve +options+ to a frozen flat Rust-facing hash for the
    # read-only FFI paths (to_html, to_markdown, to_plain_text,
    # truncate_markdown). When no options are supplied and no class-
    # level default_options has been set, return nil so the Rust side
    # skips hash-key lookups entirely and uses its hardcoded defaults—
    # the absolute fast path for one-shot renders.
    def resolve_frozen_options(options)
      return nil if options.nil? && @default_options.nil?
      case options
      when nil then default_options.to_native_hash_frozen
      when Inkmark::Options then options.to_native_hash_frozen
      when Hash then Inkmark::Options.native_hash_from(options)
      else raise TypeError, "options must be a Hash or Inkmark::Options, got #{options.class}"
      end
    end

    # Resolve +options+ to a mutable flat hash for FFI paths that
    # splice in per-call params ({chunks_by_heading}'s +:truncate+,
    # {chunks_by_size}'s +:__window+). Always builds or dups a hash—
    # the nil fast path doesn't apply because the caller will mutate
    # the result.
    def resolve_mutable_options(options)
      case options
      when nil then default_options.to_native_hash_frozen.dup
      when Inkmark::Options then options.to_native_hash_frozen.dup
      when Hash then Inkmark::Options.native_hash_from(options).dup
      else raise TypeError, "options must be a Hash or Inkmark::Options, got #{options.class}"
      end
    end
  end

  # Built-in defaults, served by {default_options} until {configure} or
  # {default_options=} installs a replacement. Frozen all the way down
  # (which also memoizes its FFI hash, see {Inkmark::Options#freeze}).
  DEFAULT_OPTIONS = Ractor.make_shareable(Inkmark::Options.new)
  private_constant :DEFAULT_OPTIONS

  # Create a new renderer for +source+.
  #
  # @param source [String, nil] markdown source; +nil+ is treated as an
  #   empty string
  # @param options [Hash, Inkmark::Options, nil] rendering options; falls back
  #   to a mutable copy of {Inkmark.default_options} when nil
  # @raise [TypeError] if +options+ is not a Hash, Inkmark::Options, or nil
  def initialize(source = nil, options: nil)
    self.source = source
    self.options = options
    @handlers = nil
  end

  # @!attribute [r] source
  #   The markdown source string that will be rendered. Always a String
  #   (never nil); a nil assignment is stored as an empty string.
  #   @return [String]
  #
  # @!attribute [r] options
  #   The rendering options for this instance.
  #   @return [Inkmark::Options]
  attr_reader :source, :options

  # Coerce the renderer to a String by returning the stored source.
  # Mirrors the wrapper idiom used by +Pathname+, +URI+, etc.: the
  # stringified form of the wrapper is its carried value. Explicit
  # renderings (HTML, Markdown, plain text) are available via
  # {#to_html}, {#to_markdown}, {#to_plain_text}, and
  # {#chunks_by_heading}.
  #
  # @return [String] the stored source, unchanged
  def to_s
    @source
  end

  # Set the markdown source.
  #
  # @param value [String, nil] markdown text; nil and non-Strings are coerced
  #   via +#to_s+
  # @return [String] the stored source
  def source=(value)
    @source = value.to_s
  end

  # Set rendering options.
  #
  # @param value [Hash, Inkmark::Options, nil] new options; nil resets to a
  #   mutable copy of {Inkmark.default_options}
  # @return [Inkmark::Options] the stored options object
  # @raise [TypeError] if +value+ is not a Hash, Inkmark::Options, or nil
  def options=(value)
    @options =
      case value
      when nil then Inkmark.default_options.dup
      when Inkmark::Options then value.dup
      when Hash then Inkmark::Options.new(value)
      else raise TypeError, "options must be a Hash or Inkmark::Options, got #{value.class}"
      end
  end

  # Register a handler block for a document element kind.
  #
  # The block receives a {Inkmark::Event} object when an element of +kind+ is
  # encountered. Handlers fire post-order—children before parents—so
  # container elements (tables, blockquotes, lists) see their children
  # populated when the handler runs.
  #
  # Multiple handlers for the same kind are supported and fire in
  # registration order. Returns +self+ for chaining.
  #
  # Trigger handlers by calling {#to_html} (render + transform) or
  # {#walk} (analysis only, no HTML output).
  #
  # @param kind [Symbol] element kind—e.g. +:heading+, +:image+, +:link+
  # @yieldparam event [Inkmark::Event]
  # @return [self]
  # @example Rewrite image sources to a CDN
  #   md.on(:image) { |img| img.dest = cdn(img.dest) }
  # @example Replace mermaid code blocks
  #   md.on(:code_block) { |c| c.html = Mermaid.render(c.source) if c.lang == "mermaid" }
  def on(kind, &block)
    (@handlers ||= {})[kind.to_sym] ||= []
    @handlers[kind.to_sym] << block
    self
  end

  # Walk the document, firing all registered handlers, without producing
  # HTML output. Use this for analysis—collecting headings, extracting
  # links, building a TOC—when you don't need to render.
  #
  # Returns +self+.
  #
  # @return [self]
  # @example Collect all links
  #   links = []
  #   md.on(:link) { |l| links << { href: l.dest, text: l.text } }
  #   md.walk
  def walk
    return self if @source.empty?
    Inkmark._native_walk(@source, @options.to_native_hash_frozen, @handlers || {})
    self
  end

  # Render the stored source to HTML using the stored options.
  #
  # When +statistics: true+ or +toc: true+ is set, the render uses a
  # single-pass entry point that also collects stats and TOC data as
  # side-effects (set as instance variables by the Rust side). Call
  # {#statistics} or {#toc} after +to_html+ to read the collected data.
  #
  # @return [String] rendered HTML, or an empty string when source is empty
  def to_html
    return "" if @source.empty?
    if @handlers
      Inkmark._native_render_with_handlers(@source, @options.to_native_hash_frozen, @handlers)
    elsif @options[:statistics] || @options[:toc] || @options[:frontmatter] || extract_requested?
      result = Inkmark._native_render_full(@source, @options.to_native_hash_frozen)
      @toc_value = if result[:toc] || result[:toc_html]
        Inkmark::Toc.new(markdown: result[:toc] || "", html: result[:toc_html] || "")
      end
      @statistics_data = result[:statistics]
      @extracts_data = result[:extracts]
      @frontmatter_raw = result[:frontmatter]
      result[:html]
    else
      Inkmark._native_to_html(@source, @options.to_native_hash_frozen)
    end
  end

  # Apply the filter pipeline and serialize back to Markdown text.
  #
  # Runs the same event-level filters as {#to_html} (controlled by the same
  # options object), then serializes the event stream to Markdown. Useful as a
  # preprocessing step in LLM or multi-renderer pipelines.
  #
  # HTML-emitting filters (+syntax_highlight+, +images: { lazy: true }+,
  # +links: { nofollow: true }+) embed raw HTML in the output when enabled—see
  # the "Markdown-to-Markdown pipeline" section in the README for guidance on
  # which filters to enable.
  #
  # @return [String] filtered Markdown, or an empty string when source is empty
  def to_markdown
    return "" if @source.empty?
    Inkmark._native_to_markdown(@source, @options.to_native_hash_frozen)
  end

  # Serialize the parsed document to plain text. Runs the same event-
  # level filters as {#to_html} (controlled by the same options object).
  # See {.to_plain_text} for output format details.
  #
  # @return [String] plain-text output, or an empty string when source is empty
  def to_plain_text
    return "" if @source.empty?
    Inkmark._native_to_plain_text(@source, @options.to_native_hash_frozen)
  end

  # Chunk the document by heading into an Array of section Hashes, with
  # filter-applied Markdown content. See {.chunks_by_heading} for the
  # output shape.
  #
  # @param truncate [Hash, nil] optional per-section truncation spec;
  #   same shape as kwargs to {#truncate_markdown} (+:chars+, +:words+,
  #   +:at+, +:marker+). Applied to every section's +:content+; counts
  #   (if +statistics: true+) are recomputed on the truncated content.
  # @return [Array<Hash>] section records
  def chunks_by_heading(truncate: nil)
    return [] if @source.empty?
    opts_hash = @options.to_native_hash_frozen.dup
    opts_hash[:truncate] = Inkmark.normalize_truncate_params(truncate) if truncate
    Inkmark._native_chunks_by_heading(@source, opts_hash)
  end

  # Split the stored document into sliding-window chunks. See
  # {.chunks_by_size} for the full parameter contract.
  #
  # @return [Array<Hash>] each +{index:, content:}+, with counts
  #   when +statistics: true+
  def chunks_by_size(chars: nil, words: nil, overlap: 0, at: :block)
    return [] if @source.empty?
    opts_hash = @options.to_native_hash_frozen.dup
    opts_hash[:__window] = Inkmark.normalize_window_params(
      chars: chars, words: words, overlap: overlap, at: at
    )
    Inkmark._native_chunks_by_size(@source, opts_hash)
  end

  # Truncate the stored document. See {.truncate_markdown} for the full
  # parameter contract.
  #
  # @return [String] truncated Markdown, or the source unchanged when
  #   it already fits
  def truncate_markdown(chars: nil, words: nil, at: :block, marker: "…")
    return "" if @source.empty?
    params = Inkmark.normalize_truncate_params(
      chars: chars, words: words, at: at, marker: marker
    )
    Inkmark._native_truncate_markdown(@source, params, @options.to_native_hash_frozen)
  end

  # Return the table of contents as a {Inkmark::Toc} value object,
  # exposing +#to_markdown+ / +#to_html+ / +#to_s+ (markdown). Returns
  # +nil+ when no TOC was requested (neither +toc+, +statistics+, nor
  # +extract: { headings: true }+ is set).
  #
  # Collected during {#to_html} as a side-effect of the single-pass
  # render. If +to_html+ hasn't been called yet, calling this triggers
  # it.
  #
  # @return [Inkmark::Toc, nil]
  # @example
  #   g.toc.to_markdown  # "- [Intro](#intro)\n..."
  #   g.toc.to_html      # "<ul><li>..."
  #   puts g.toc         # prints markdown form (via to_s)
  def toc
    return nil unless toc_surface_requested?
    to_html unless defined?(@toc_value) && @toc_value
    @toc_value
  end

  # Return the collected document statistics as a Hash, or +nil+ when
  # neither +statistics+ nor +toc+ is enabled.
  #
  # When +statistics: true+, the full hash includes language detection,
  # character/word counts, code block count, and image/link arrays.
  # When only +toc: true+, a lightweight hash with +heading_count+ is
  # returned.
  #
  # Collected during {#to_html}. Calling this before +to_html+ triggers
  # the render.
  #
  # @return [Hash, nil]
  def statistics
    return nil unless @options[:statistics] || @options[:toc]
    to_html unless @statistics_data
    @statistics_data
  end

  # Return structured extracts for the element kinds requested via
  # +extract: { ... }+, or +nil+ when no kinds were requested.
  #
  # The returned Hash is keyed by the same symbols you passed in
  # (+:images+, +:links+, +:code_blocks+, +:headings+,
  # +:footnote_definitions+); each value is an Array of record Hashes
  # including a +:byte_range+ Range for slicing the original source.
  #
  # +toc: true+ auto-enables +extract[:headings]+—the heading walk is
  # shared, so you get the structured view for free.
  #
  # Collected during {#to_html} as a side-effect of the single-pass
  # render. Calling this before +to_html+ triggers the render.
  #
  # @return [Hash, nil]
  # @example
  #   md = Inkmark.new(source, options: { extract: { images: true } })
  #   md.extracts[:images]
  #   #=> [{ src: "cat.png", alt: "cat", title: "", byte_range: 12...28 }]
  def extracts
    return nil unless extract_requested?
    to_html unless @extracts_data
    @extracts_data
  end

  # Return the parsed frontmatter as a Hash, or +nil+ when the document
  # has no frontmatter block or the +frontmatter+ option is not enabled.
  #
  # The raw YAML text is extracted by Rust during the event walk;
  # parsing uses Ruby's stdlib +YAML.safe_load+ so all standard YAML
  # types (strings, numbers, arrays, nested hashes) are supported.
  # Psych is loaded here, on first use, rather than with the gem: front
  # matter is opt-in, and this keeps Psych's load time and its own
  # constants out of processes that never enable it.
  #
  # @return [Hash, nil] parsed frontmatter or nil
  # @example
  #   md = Inkmark.new("---\ntitle: Hello\n---\n\n# Content",
  #                   options: { frontmatter: true })
  #   md.frontmatter  #=> { "title" => "Hello" }
  def frontmatter
    return @frontmatter if defined?(@frontmatter)
    return @frontmatter = nil unless @options[:frontmatter]
    to_html unless @frontmatter_raw
    return @frontmatter = nil unless @frontmatter_raw

    require "yaml" unless defined?(::YAML) # audition:disable runtime-require
    @frontmatter = YAML.safe_load(@frontmatter_raw)
  end

  private

  # True when any request triggers the TOC walk—`toc: true`,
  # `statistics: true`, or `extract: { headings: true }`. Used by
  # {#toc} and {#toc_to_html} to decide whether to surface their
  # computed value to the caller.
  def toc_surface_requested?
    return true if @options[:toc] || @options[:statistics]
    extract = @options[:extract]
    extract.is_a?(Hash) && extract[:headings] == true
  end

  # True when the user explicitly asked for any extract kind, OR when
  # `toc: true` implicitly pulls headings into extracts. Matches the
  # mutual trigger implemented on the Rust side.
  def extract_requested?
    return true if @options[:toc]
    extract = @options[:extract]
    extract.is_a?(Hash) && extract.any? { |_, v| v }
  end
end

# Warm the class-level memo on the main Ractor while loading, so worker
# Ractors only ever read it (a first write from a worker would raise
# Ractor::IsolationError).
Inkmark.highlight_themes
