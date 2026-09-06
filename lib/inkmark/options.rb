# frozen_string_literal: true

class Inkmark
  # Typed hash of Inkmark rendering options with a known key set.
  # Unknown keys raise ArgumentError at every write path.
  #
  # Nested policy hashes—+:headings+, +:images+, +:links+—group related
  # options together and deep-merge over defaults when set, so users
  # can tweak one sub-key without clobbering the others.
  #
  # The meta +:preset+ option (accepted in {.new} and {.native_hash_from})
  # selects a named bundle from {PRESETS} applied before the rest of
  # the overrides. +:gfm+ is the default preset; see {PRESETS} for the
  # full list.
  #
  # @example Preset + per-app overrides
  #   Inkmark::Options.new(
  #     preset: :recommended,
  #     links:  { allowed_hosts: ["*.example.com"] }
  #   )
  class Options
    # Per-element-policy schemas. Each entry is +{ default:, types: }+; the
    # validators use +types+ for type checking and +default+ to seed fresh
    # nested hashes. Keep in sync with {NESTED_TO_FLAT}.
    #
    # Every constant in this class is frozen all the way down, not just
    # at the top level: a non-main Ractor may only read constants whose
    # whole object graph is frozen (+Ractor.shareable?+), so the inner
    # Hashes and Arrays are frozen explicitly and the larger nested
    # tables go through +Ractor.make_shareable+.
    HEADINGS_SCHEMA = {
      attributes: {default: false, types: [TrueClass, FalseClass].freeze}.freeze,
      ids: {default: false, types: [TrueClass, FalseClass].freeze}.freeze
    }.freeze

    IMAGES_SCHEMA = {
      lazy: {default: false, types: [TrueClass, FalseClass].freeze}.freeze,
      allowed_hosts: {default: nil, types: [NilClass, Array].freeze}.freeze,
      allowed_schemes: {default: nil, types: [NilClass, Array].freeze}.freeze
    }.freeze

    LINKS_SCHEMA = {
      autolink: {default: false, types: [TrueClass, FalseClass].freeze}.freeze,
      nofollow: {default: false, types: [TrueClass, FalseClass].freeze}.freeze,
      allowed_hosts: {default: nil, types: [NilClass, Array].freeze}.freeze,
      allowed_schemes: {default: nil, types: [NilClass, Array].freeze}.freeze
    }.freeze

    # Registry of nested hash options => their schemas. Iterated by the
    # validator and native-hash flattener to keep the three element-policy
    # groupings uniform.
    NESTED_SCHEMAS = {
      headings: HEADINGS_SCHEMA,
      images: IMAGES_SCHEMA,
      links: LINKS_SCHEMA
    }.freeze

    # Map from +(parent, child)+ user-facing keys to the flat key name the
    # Rust side reads. Used by {#to_native_hash} / {#to_native_hash_frozen}
    # to serialize the user-shaped hash into the FFI wire format.
    NESTED_TO_FLAT = Ractor.make_shareable({
      [:headings, :attributes] => :heading_attributes,
      [:headings, :ids] => :heading_ids,
      [:images, :lazy] => :lazy_images,
      [:images, :allowed_hosts] => :allowed_image_hosts,
      [:images, :allowed_schemes] => :allowed_image_schemes,
      [:links, :autolink] => :autolink,
      [:links, :nofollow] => :nofollow_external_links,
      [:links, :allowed_hosts] => :allowed_link_hosts,
      [:links, :allowed_schemes] => :allowed_link_schemes
    })

    # Build a frozen defaults hash for a nested schema from its +default+
    # entries.
    def self.schema_defaults(schema)
      schema.each_with_object({}) { |(k, v), h| h[k] = v[:default] }.freeze
    end

    # Default values for every option. Top-level keys are user-facing; nested
    # element-policy groups (+headings+, +images+, +links+) hold their own
    # default hashes built from {NESTED_SCHEMAS}.
    DEFAULTS = {
      # GFM conformance bundle. Enables pulldown-cmark's ENABLE_GFM and the
      # four core GFM extensions. Individual extensions can still be toggled
      # off after setting gfm: true.
      gfm: true,

      # GFM "Disallowed Raw HTML" extension. When +gfm+ and +raw_html+ are
      # both true, escapes the leading +<+ of nine unsafe tag names
      # (title, textarea, style, xmp, iframe, noembed, noframes, script,
      # plaintext). Required for GFM conformance; no effect when
      # +raw_html+ is false.
      gfm_tag_filter: true,

      # GFM pipe tables with optional column-alignment markers.
      tables: true,

      # GFM strikethrough: +~~text~~+ → +<del>+.
      strikethrough: true,

      # GFM task lists: +- [ ]+ and +- [x]+ → disabled checkboxes.
      tasklists: true,

      # Footnote references and definitions.
      footnotes: true,

      # Pass raw HTML tags through unescaped. Off by default for
      # untrusted-input safety. When true, the caller is fully responsible
      # for sanitizing output—Inkmark does not sanitize beyond the narrow
      # GFM tagfilter. Always run the output through a dedicated sanitizer
      # (Sanitize, Loofah, rails-html-sanitizer) for untrusted content.
      raw_html: false,

      # Smart punctuation: ASCII quotes/dashes/ellipses → typographic forms.
      smart_punctuation: false,

      # Heading-related options. +:attributes+ enables +# Title {#id .klass}+
      # inline attribute syntax; +:ids+ auto-generates an +id+ on every
      # heading from its text (slug). User-supplied ids from +attributes+
      # are preserved when +:ids+ fills the rest in.
      headings: schema_defaults(HEADINGS_SCHEMA),

      # Image-related options. +:lazy+ adds +loading="lazy" decoding="async"+
      # to every +<img>+. +:allowed_hosts+ is a glob allowlist for
      # +<img src>+ hostnames (http/https); non-matching images drop to alt
      # text. +:allowed_schemes+ is a URL-scheme allowlist for image URLs.
      # Both allowlists default to +nil+ (no filtering); set +[]+ to
      # deny-all-external.
      images: schema_defaults(IMAGES_SCHEMA),

      # Link-related options. +:autolink+ turns bare URLs and emails into
      # clickable links. +:nofollow+ adds +rel="nofollow noopener"+ to
      # external +<a>+ tags. +:allowed_hosts+ / +:allowed_schemes+ are
      # glob / scheme allowlists for +<a href>+ (same semantics as the
      # image versions). Relative/anchor/mailto URLs are never filtered.
      links: schema_defaults(LINKS_SCHEMA),

      # Replace gemoji-style +:shortcode:+ sequences with the emoji
      # character. Unknown codes and codes inside code blocks are preserved.
      emoji_shortcodes: false,

      # Server-side syntax highlighting for fenced code blocks with a
      # language tag. Uses syntect with CSS class output—pair with
      # {Inkmark.highlight_css} for the theme stylesheet.
      syntax_highlight: false,

      # Treat every single newline in a paragraph as a hard line break
      # (+<br />+). Default is soft-break (single +\n+ → space).
      hard_wrap: false,

      # Collect a table of contents from headings. When set, {Inkmark#toc}
      # returns a {Inkmark::Toc} value object (+#to_markdown+ / +#to_html+ /
      # +#to_s+). Implicitly enables +headings[:ids]+ in the rendered HTML
      # so TOC anchor hrefs have matching targets. Also populates
      # {Inkmark#statistics} with +heading_count+.
      #
      # Accepts +true+ / +false+ for simple enable/disable, or a Hash with a
      # +:depth+ key to limit which heading levels appear in the rendered
      # TOC. +toc: { depth: 3 }+ renders h1–h3 only; +toc: {}+ or
      # +toc: true+ renders all levels. Depth filtering affects only the
      # rendered TOC; +heading_count+, +extracts[:headings]+, and
      # +chunks_by_heading+ still see every heading.
      toc: false,

      # Full document statistics: language detection, character/word counts,
      # and +*_count+ fields for headings, code blocks, images, links, and
      # footnote definitions. For structured arrays of records, use
      # {extract}. Implies +toc+ and +headings[:ids]+.
      statistics: false,

      # Opt into structured extraction of specific element kinds. Pass a
      # Hash with any of +:images+, +:links+, +:code_blocks+, +:headings+,
      # +:footnote_definitions+ set to +true+. {Inkmark#extracts} then
      # returns a Hash keyed by the requested kinds, each carrying an Array
      # of record Hashes with a +:byte_range+. +nil+ (default) disables
      # extraction. +extract: { headings: true }+ and +toc: true+ trigger
      # each other—one heading walk powers both surfaces.
      extract: nil,

      # Math: +$inline$+ and +$$display$$+ blocks → +<code class="language-math">+.
      math: false,

      # Definition lists: +term\n: definition+ → +<dl>+.
      definition_list: false,

      # Superscript: +^text^+ → +<sup>+.
      superscript: false,

      # Subscript: +~text~+ → +<sub>+ (conflicts with strikethrough—enable
      # only one).
      subscript: false,

      # Wiki-style links: +[[Page]]+ and +[[Page|label]]+ → +<a>+.
      wikilinks: false,

      # Frontmatter: YAML metadata at the start of the document. Recognized
      # +---\nkey: value\n---+ block is stripped from rendered output and
      # exposed as a Hash via {Inkmark#frontmatter}.
      frontmatter: false
    }.freeze

    # Per-key class allowlist. Keys absent from this hash inherit their
    # allowed classes from {DEFAULTS}: a boolean default allows
    # +TrueClass+/+FalseClass+, a +nil+ default allows +NilClass+, and so
    # on. Entries here declare options whose default doesn't fully describe
    # the accepted type set (nil-default-but-Array-when-set, polymorphic
    # +toc+, nested-hash element-policy groups).
    TYPES = {
      extract: [NilClass, Hash].freeze,
      toc: [TrueClass, FalseClass, Hash].freeze,
      headings: [Hash].freeze,
      images: [Hash].freeze,
      links: [Hash].freeze
    }.freeze

    # Element kinds accepted inside +extract: { ... }+. Mirrors the match
    # arms in Rust +stats::to_extracts_hash+—changing one means changing
    # the other.
    EXTRACT_KINDS = %i[
      images
      links
      code_blocks
      headings
      footnote_definitions
    ].freeze

    # Named bundles of option settings. Pass +preset: :name+ in the
    # options hash and the preset's values are applied first; any other
    # keys override (deep-merging for nested element-policy hashes).
    #
    # - +:gfm+ (the default applied by {#initialize}) — CommonMark +
    #   core GFM extensions (tables, strikethrough, tasklists, footnotes,
    #   tagfilter). Conservative, matches the render profile of every
    #   other major GFM engine.
    # - +:commonmark+ — strict CommonMark, no GFM extensions.
    # - +:recommended+ — opinionated bundle for modern web content.
    #   Enables smart punctuation, auto heading IDs, lazy-loading images,
    #   autolinks, +rel="nofollow noopener"+ on external links, URL
    #   scheme allowlists for links and images, emoji shortcodes, syntax
    #   highlighting, hard wraps, and frontmatter. Recommended starting
    #   point for apps; override individual options to tune.
    # - +:trusted+ — +:recommended+ with raw HTML pass-through enabled.
    #   The GFM tagfilter stays on. **Dangerous.** Use only for content
    #   the caller fully trusts (internal team-authored docs). The
    #   caller is fully responsible for sanitizing output.
    PRESETS = Ractor.make_shareable({
      commonmark: {
        gfm: false,
        gfm_tag_filter: false,
        tables: false,
        strikethrough: false,
        tasklists: false,
        footnotes: false
      },

      gfm: {
        gfm: true,
        gfm_tag_filter: true,
        tables: true,
        strikethrough: true,
        tasklists: true,
        footnotes: true
      },

      recommended: {
        gfm: true,
        gfm_tag_filter: true,
        tables: true,
        strikethrough: true,
        tasklists: true,
        footnotes: true,
        raw_html: false,
        smart_punctuation: true,
        headings: {attributes: false, ids: true},
        images: {lazy: true, allowed_schemes: ["http", "https"]},
        links: {autolink: true, nofollow: true, allowed_schemes: ["http", "https", "mailto"]},
        emoji_shortcodes: true,
        syntax_highlight: true,
        hard_wrap: true,
        frontmatter: true
      },

      trusted: {
        gfm: true,
        gfm_tag_filter: true,
        tables: true,
        strikethrough: true,
        tasklists: true,
        footnotes: true,
        raw_html: true,
        smart_punctuation: true,
        headings: {attributes: false, ids: true},
        images: {lazy: true, allowed_schemes: ["http", "https"]},
        links: {autolink: true, nofollow: true, allowed_schemes: ["http", "https", "mailto"]},
        emoji_shortcodes: true,
        syntax_highlight: true,
        hard_wrap: true,
        frontmatter: true
      }
    })

    # Preset applied by {#initialize} when the caller doesn't pass
    # +preset:+. +:gfm+ matches {DEFAULTS}, so the default constructor
    # is equivalent to "CommonMark + core GFM, nothing else".
    DEFAULT_PRESET = :gfm

    # Build a new Options instance with defaults from {DEFAULTS} plus any
    # overrides applied on top. Nested hash values (for +:headings+,
    # +:images+, +:links+) are deep-merged over the defaults—users only
    # need to pass the sub-keys they care about.
    #
    # @param overrides [Hash{Symbol => Object}] option keys and values to
    #   override against the defaults. Every key must be present in
    #   {DEFAULTS} or +ArgumentError+ is raised.
    # @option overrides [Boolean] :gfm (true) GFM conformance mode +
    #   bundle-enable tables, strikethrough, tasklists, and footnotes.
    # @option overrides [Boolean] :tables (true) GFM pipe tables.
    # @option overrides [Boolean] :strikethrough (true) +~~text~~+.
    # @option overrides [Boolean] :tasklists (true) +- [ ]+ / +- [x]+.
    # @option overrides [Boolean] :footnotes (true) +[^1]+ / +[^1]: body+.
    # @option overrides [Boolean] :raw_html (false) Pass raw HTML through.
    # @option overrides [Boolean] :smart_punctuation (false) Typographic
    #   quotes/dashes/ellipses.
    # @option overrides [Hash] :headings ({attributes: false, ids: false})
    #   Heading-related policy. Sub-keys: +:attributes+ (inline
    #   +{#id .klass}+ syntax), +:ids+ (auto-generate slug ids).
    # @option overrides [Hash] :images ({lazy: false, allowed_hosts: nil,
    #   allowed_schemes: nil}) Image-related policy. Sub-keys: +:lazy+
    #   (+loading="lazy"+), +:allowed_hosts+ (glob allowlist), +:allowed_schemes+.
    # @option overrides [Hash] :links ({autolink: false, nofollow: false,
    #   allowed_hosts: nil, allowed_schemes: nil}) Link-related policy.
    #   Sub-keys: +:autolink+, +:nofollow+ (external +rel+), +:allowed_hosts+,
    #   +:allowed_schemes+.
    # @option overrides [Boolean] :emoji_shortcodes (false) +:rocket:+ → 🚀.
    # @option overrides [Boolean] :syntax_highlight (false) Server-side
    #   syntect highlighting for fenced code blocks.
    # @option overrides [Boolean] :hard_wrap (false) Every +\n+ → +<br />+.
    # @option overrides [Boolean, Hash] :toc (false) Collect TOC.
    #   +true+ / +{}+ includes all heading levels; +{ depth: N }+ limits to
    #   h1..hN (1..6).
    # @option overrides [Boolean] :statistics (false) Full document stats.
    # @option overrides [Hash, nil] :extract (nil) Structured element
    #   extraction. Keys: +:images+, +:links+, +:code_blocks+, +:headings+,
    #   +:footnote_definitions+.
    # @option overrides [Boolean] :math (false)
    # @option overrides [Boolean] :definition_list (false)
    # @option overrides [Boolean] :superscript (false)
    # @option overrides [Boolean] :subscript (false)
    # @option overrides [Boolean] :wikilinks (false)
    # @option overrides [Boolean] :frontmatter (false) Parse YAML
    #   frontmatter and expose via {Inkmark#frontmatter}.
    # @option overrides [Symbol] :preset (:gfm) Named bundle of option
    #   settings applied before the rest of +overrides+. See {PRESETS}
    #   for the available names. Every other key in +overrides+ takes
    #   precedence over the preset (nested hashes deep-merge).
    # @raise [ArgumentError] if any key in +overrides+ is unknown, any
    #   nested sub-key is unknown, any value has the wrong type, or
    #   +preset:+ is not a known preset name.
    # @example With defaults
    #   Inkmark::Options.new[:tables]  #=> true
    # @example Deep-merge over nested defaults
    #   opts = Inkmark::Options.new(images: { lazy: true })
    #   opts[:images]  #=> { lazy: true, allowed_hosts: nil, allowed_schemes: nil }
    # @example Preset + override
    #   opts = Inkmark::Options.new(preset: :recommended, smart_punctuation: false)
    #   opts[:smart_punctuation]  #=> false  (override wins)
    #   opts[:syntax_highlight]   #=> true   (kept from :recommended)
    def initialize(overrides = {})
      @values = dup_with_nested(DEFAULTS)
      @toc_depth = nil
      @frozen_native_hash = nil
      apply_overrides!(overrides, default_preset: DEFAULT_PRESET)
    end

    # Read an option by key. Nested element-policy keys return the nested
    # hash as a live reference—mutating it directly bypasses cache
    # invalidation; prefer the setter.
    #
    # @param key [Symbol] a key from {DEFAULTS}
    # @return [Object] the current value for that key
    # @raise [KeyError] if +key+ is not present in {DEFAULTS}
    def [](key)
      @values.fetch(key)
    end

    # Write an option by key. For nested element-policy keys (+:headings+,
    # +:images+, +:links+) the hash is deep-merged over the current value,
    # so callers may pass only the sub-keys they want to change.
    #
    # @param key [Symbol] a key from {DEFAULTS}
    # @param value [Object] the new value
    # @return [Object] the value that was written (post-merge for nested
    #   hashes; the input value as-is otherwise)
    # @raise [ArgumentError] if +key+ is unknown, or the value (or any
    #   nested sub-value) has the wrong type
    # @raise [FrozenError] if this instance is frozen (as
    #   {Inkmark.default_options} always is)
    def []=(key, value)
      if frozen?
        raise FrozenError.new(
          "can't modify frozen #{self.class}: use Inkmark.configure to change " \
          "the process-wide defaults, or dup for a mutable copy",
          receiver: self
        )
      end
      validate_key!(key)
      # Deep-merge partial nested-hash overrides (+:headings+,
      # +:images+, +:links+) so callers pass only the sub-keys they
      # care about; non-Hash values fall through to validate_value!
      # and raise there.
      value = @values[key].merge(value) if NESTED_SCHEMAS.key?(key) && value.is_a?(Hash)
      validate_value!(key, value)
      @values[key] = value
      # Sugar: +toc: { depth: N }+ normalizes to +toc: true+ plus the
      # depth stashed in +@toc_depth+ (not a user-facing option).
      if key == :toc && value.is_a?(Hash)
        @values[:toc] = true
        @toc_depth = value[:depth]
      end
      @frozen_native_hash = nil
    end

    # Return a plain user-shaped Hash copy of the current option values.
    # Nested element-policy groups are returned as nested Hashes,
    # mirroring the input shape accepted by {#initialize}.
    #
    # @return [Hash{Symbol => Object}]
    def to_h
      dup_with_nested(@values)
    end

    # Return a Rust-facing flat Hash: nested element-policy hashes are
    # expanded into their flat Rust keys via {NESTED_TO_FLAT}, and the
    # internal +@toc_depth+ is injected when set. Used by the FFI layer.
    #
    # @return [Hash{Symbol => Object}] fresh mutable hash; callers that
    #   add per-call params (truncate, window, etc.) mutate this hash
    # @api private
    def to_native_hash
      build_native_hash
    end

    # Memoized frozen variant of {#to_native_hash} used by the hot-path
    # FFI calls that don't need to add per-call params. The cache is
    # invalidated in {#[]=} and {#initialize_copy}.
    #
    # The hash is frozen all the way down, as a copy: nested Arrays and
    # Hashes are duplicated before freezing so caller-supplied values
    # (an +allowed_hosts+ Array, say) stay mutable in the caller's hands.
    # A deeply frozen memo is what lets a frozen +Options+ be shared
    # across Ractors.
    #
    # @return [Hash{Symbol => Object}] deeply frozen, shared across calls
    #   until a mutation invalidates it
    # @api private
    def to_native_hash_frozen
      @frozen_native_hash ||= deep_frozen_copy(build_native_hash)
    end

    # Return a new Options instance with +other+'s values applied on top.
    # Nested element-policy hashes deep-merge; top-level values replace.
    # Accepts +preset:+ on +other+ to re-apply a named preset before the
    # other overrides (unlike {#initialize}, no default preset is applied
    # when +other+ omits +preset:+—the receiver's state is preserved).
    #
    # @param other [Inkmark::Options, Hash] source of overriding values
    # @return [Inkmark::Options] merged result; neither receiver nor +other+ is mutated
    def merge(other)
      other_hash = other.is_a?(Inkmark::Options) ? other.to_h : other
      merged = dup
      merged.send(:apply_overrides!, other_hash, default_preset: nil)
      merged
    end

    # Compare by value equality (user-shaped view).
    def ==(other)
      other.class == self.class && to_h == other.to_h
    end
    alias_method :eql?, :==

    # Freeze this instance. The memoized FFI hash is computed first, while
    # the instance is still mutable, so {#to_native_hash_frozen} never has
    # to write to a frozen object. +Ractor.make_shareable+ calls +freeze+
    # on every object it visits, which is what makes a shared
    # {Inkmark.default_options} renderable from any Ractor.
    #
    # @return [self]
    def freeze
      to_native_hash_frozen
      super
    end

    # Duplicate this instance, deep-copying the internal values hash so the
    # clone is fully independent from the original.
    def initialize_copy(orig)
      super
      @values = dup_with_nested(orig.instance_variable_get(:@values))
      @toc_depth = orig.instance_variable_get(:@toc_depth)
      @frozen_native_hash = nil
    end

    # Reader and writer for every option key. The writer routes through
    # {#[]=} so key validation and (for nested groups) deep-merge apply
    # uniformly.
    #
    # Generated from source strings rather than +define_method+ blocks:
    # a block-defined method carries its Proc, and Ruby refuses to call
    # such a method from a non-main Ractor ("defined with an un-shareable
    # Proc in a different Ractor"). Plain +def+ bodies have no such
    # baggage. +key+ is always a Symbol from {DEFAULTS}, so interpolating
    # it is safe.
    DEFAULTS.each_key do |key|
      class_eval(<<~RUBY, __FILE__, __LINE__ + 1)
        def #{key}                # def tables
          @values[:#{key}]        #   @values[:tables]
        end                       # end

        def #{key}=(value)        # def tables=(value)
          self[:#{key}] = value   #   self[:tables] = value
        end                       # end
      RUBY
    end

    private

    # Apply a hash of override values, handling the pseudo-option
    # +:preset+ by expanding it into its PRESETS entry first. Called
    # from {#initialize} (which passes +default_preset: :gfm+ so bare
    # +Options.new+ gets the GFM preset) and {#merge} (which passes
    # +default_preset: nil+ so the receiver's state is preserved when
    # the caller doesn't specify a preset).
    def apply_overrides!(overrides, default_preset:)
      overrides = overrides.to_h
      preset_name = overrides.fetch(:preset, default_preset)
      overrides = overrides.except(:preset)

      if preset_name
        preset = PRESETS.fetch(preset_name) do
          raise ArgumentError,
            "unknown preset: #{preset_name.inspect}; expected one of #{PRESETS.keys.inspect}"
        end
        preset.each { |k, v| self[k] = v }
      end

      overrides.each { |k, v| self[k] = v }
    end

    # Shallow-dup a hash but deep-dup any one-level nested hashes, so
    # the caller can mutate nested entries without aliasing back into
    # +source+. Used to seed +@values+ from DEFAULTS, to snapshot
    # +@values+ for {#to_h}, and to fork +@values+ in
    # {#initialize_copy}.
    def dup_with_nested(source)
      source.each_with_object({}) do |(k, v), h|
        h[k] = v.is_a?(Hash) ? v.dup : v
      end
    end

    # Delegate to the class-method validators so both +[]=+ (instance)
    # and {.native_hash_from} (class-level fast path) share one source
    # of truth for validation rules and error messages.
    def validate_key!(key) = self.class.send(:validate_key!, key)
    def validate_value!(key, value) = self.class.send(:validate_value!, key, value)

    # Return a frozen copy of +value+ with every nested Hash, Array, and
    # String frozen too. Scalars (booleans, nil, Integers, Symbols) are
    # returned as is. Only the container shapes {#build_native_hash} can
    # produce are handled.
    def deep_frozen_copy(value)
      case value
      when Hash then value.transform_values { |v| deep_frozen_copy(v) }.freeze
      when Array then value.map { |v| deep_frozen_copy(v) }.freeze
      when String then -value
      else value
      end
    end

    # Build the Rust-facing flat hash: nested element-policy hashes expand
    # into their flat keys via {NESTED_TO_FLAT}; the internal +@toc_depth+
    # is injected when set.
    def build_native_hash
      h = {}
      @values.each do |key, value|
        if NESTED_SCHEMAS.key?(key)
          value.each do |sub_key, sub_value|
            h[NESTED_TO_FLAT.fetch([key, sub_key])] = sub_value
          end
        else
          h[key] = value
        end
      end
      h[:toc_depth] = @toc_depth unless @toc_depth.nil?
      h
    end

    # Pure functions of class-level constants (DEFAULTS, TYPES,
    # NESTED_SCHEMAS, EXTRACT_KINDS). Called from both +#[]=+ (via
    # instance delegates above) and {.native_hash_from} (the
    # class-method fast path that bypasses Options allocation for
    # one-shot callers).

    class << self
      private

      def validate_key!(key)
        return if DEFAULTS.key?(key)
        raise ArgumentError, "unknown Inkmark option: #{key.inspect}"
      end

      def validate_value!(key, value)
        allowed = TYPES[key] || default_types_for(key)
        unless allowed.any? { |klass| value.is_a?(klass) }
          raise ArgumentError,
            "invalid value for #{key}: got #{value.class} (#{value.inspect}), " \
            "expected one of #{allowed.inspect}"
        end
        validate_extract_hash!(value) if key == :extract && value.is_a?(Hash)
        validate_toc_hash!(value) if key == :toc && value.is_a?(Hash)
        validate_nested_hash!(key, value) if NESTED_SCHEMAS.key?(key)
      end

      def validate_extract_hash!(hash)
        hash.each do |kind, enabled|
          unless EXTRACT_KINDS.include?(kind)
            raise ArgumentError,
              "unknown extract kind: #{kind.inspect}; " \
              "expected one of #{EXTRACT_KINDS.inspect}"
          end
          unless enabled == true || enabled == false
            raise ArgumentError,
              "invalid value for extract[#{kind.inspect}]: got #{enabled.class} (#{enabled.inspect}), " \
              "expected true or false"
          end
        end
      end

      def validate_toc_hash!(hash)
        unknown = hash.keys - [:depth]
        unless unknown.empty?
          raise ArgumentError,
            "unknown toc key(s): #{unknown.inspect}; expected :depth"
        end
        depth = hash[:depth]
        return if depth.nil?
        unless depth.is_a?(Integer) && (1..6).cover?(depth)
          raise ArgumentError,
            "invalid value for toc depth: got #{depth.inspect}, expected nil or Integer 1..6"
        end
      end

      def validate_nested_hash!(key, hash)
        schema = NESTED_SCHEMAS[key]
        unknown = hash.keys - schema.keys
        unless unknown.empty?
          raise ArgumentError,
            "unknown #{key} key(s): #{unknown.inspect}; " \
            "expected one of #{schema.keys.inspect}"
        end
        hash.each do |sub_key, sub_value|
          types = schema[sub_key][:types]
          unless types.any? { |klass| sub_value.is_a?(klass) }
            raise ArgumentError,
              "invalid value for #{key}[#{sub_key.inspect}]: got #{sub_value.class} " \
              "(#{sub_value.inspect}), expected one of #{types.inspect}"
          end
        end
      end

      def default_types_for(key)
        case DEFAULTS[key]
        when true, false then [TrueClass, FalseClass]
        when nil then [NilClass]
        else [DEFAULTS[key].class]
        end
      end
    end

    # Precomputed flat Rust-facing hash per preset. Built once at load
    # time by running each preset through +Options.new(preset: name)+
    # and memoizing the resulting +to_native_hash_frozen+. Used by the
    # class-method fast paths in {Inkmark} to short-circuit the
    # +options: { preset: :name }+ call pattern, which would otherwise
    # build a fresh +Options+ instance (seed defaults, 6–14 +[]=+
    # with validation, +build_native_hash+) on every call. The cached
    # hashes are deeply frozen and safe to share across threads and
    # Ractors.
    PRESETS_NATIVE_HASH = Ractor.make_shareable(
      PRESETS.keys.each_with_object({}) do |name, h|
        h[name] = new(preset: name).to_native_hash_frozen
      end
    )

    # Build a Rust-facing flat hash from +overrides+ without allocating
    # an +Options+ instance or walking +build_native_hash+. Starts from
    # the cached preset native hash and applies user overrides directly
    # to the flat form; nested element-policy hashes (+:headings+,
    # +:images+, +:links+) are flattened via {NESTED_TO_FLAT}; +toc:
    # Hash+ is expanded to +toc: true, toc_depth: N+.
    #
    # Semantically equivalent to
    # +Options.new(overrides).to_native_hash_frozen+ but bypasses the
    # Options object. Validation matches +[]=+ exactly—same
    # ArgumentErrors for unknown keys, wrong types, unknown sub-keys,
    # out-of-range toc depth, and unknown extract kinds.
    #
    # @api private
    class << self
      def native_hash_from(overrides)
        overrides = overrides.to_h
        preset_name = overrides[:preset] || DEFAULT_PRESET
        cached = PRESETS_NATIVE_HASH[preset_name]
        unless cached
          raise ArgumentError,
            "unknown preset: #{preset_name.inspect}; expected one of #{PRESETS.keys.inspect}"
        end

        # Fast-fast path: only :preset (or nothing) → return cached frozen hash.
        non_preset_keys = overrides.size - (overrides.key?(:preset) ? 1 : 0)
        return cached if non_preset_keys.zero?

        h = cached.dup
        toc_depth = h[:toc_depth]

        overrides.each do |key, value|
          next if key == :preset
          validate_key!(key)
          validate_value!(key, value)

          case key
          when :headings, :images, :links
            # value is a Hash (validated); flatten each sub-key via
            # NESTED_TO_FLAT. The flat-hash representation of the final
            # state is equivalent to the deep-merged nested representation,
            # since the preset-cached base already has all sub-keys present.
            value.each do |sub_key, sub_value|
              h[NESTED_TO_FLAT.fetch([key, sub_key])] = sub_value
            end
          when :toc
            if value.is_a?(Hash)
              h[:toc] = true
              toc_depth = value[:depth]
            else
              h[:toc] = value
            end
          else
            h[key] = value
          end
        end

        if toc_depth.nil?
          h.delete(:toc_depth)
        else
          h[:toc_depth] = toc_depth
        end
        h.freeze
      end
    end
  end
end
