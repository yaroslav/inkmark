# frozen_string_literal: true

class Inkmark
  # Represents a parsed document element passed to {Inkmark#on} handlers.
  #
  # ## Reading fields
  #
  # Every event exposes:
  # - {#kind}           the element type as a Symbol
  # - {#text}           the concatenated plain text of all descendant text nodes
  # - {#depth}          the nesting depth (0 = top-level block)
  # - {#parent_kind}    the kind of the immediate parent, or +nil+ at the root
  # - {#ancestor_kinds} an array of ancestor kinds from parent to root
  # - {#children}       child Event objects (containers only; empty for leaves)
  # - {#children_of}    children filtered by kind
  #
  # ## Mutating elements
  #
  # Set any mutable attribute inside a handler to transform the element's output:
  # - {#html=}  replace the element's entire output with a raw HTML string
  # - {#dest=}  rewrite the URL on +:link+ or +:image+ elements
  # - {#title=} rewrite the title on +:link+ or +:image+ elements
  # - {#level=} change the heading level (1–6) on +:heading+ elements
  # - {#id=}    change the heading +id+ attribute on +:heading+ elements
  #
  # Call {#delete} to suppress the element entirely (nothing emitted).
  #
  # ## Kind reference
  #
  # Handlers fire post-order: children are processed before parents, so
  # container elements see their children already available.
  #
  # ### Container kinds
  #
  # Containers have matching Start/End events. Their handler fires after
  # all children have been processed.
  #
  # #### +:heading+
  # - +text+     plain text of the heading (emoji already resolved)
  # - +level+    heading level 1–6 (mutable via +level=+)
  # - +id+       slug from +headings: { ids: true }+ option, otherwise +nil+ (mutable via +id=+)
  # - +children+ inline child nodes (text, strong, emphasis, link, code, …)
  # - Mutable: +level=+, +id=+, +html=+
  #
  # #### +:paragraph+
  # - +text+     plain text content
  # - +children+ inline child nodes
  # - Mutable: +html=+
  #
  # #### +:blockquote+
  # - +text+     plain text of all descendant text nodes
  # - +children+ block child nodes (paragraphs, nested blockquotes, …)
  # - Mutable: +html=+
  #
  # #### +:list+
  # - +children+ +:list_item+ elements
  # - Mutable: +html=+
  #
  # #### +:ordered_list+
  # - +children+ +:list_item+ elements
  # - Mutable: +html=+
  #
  # #### +:list_item+
  # - +text+     plain text content
  # - +children+ block or inline child nodes
  # - Mutable: +html=+
  #
  # #### +:code_block+
  # - +text+ / +source+ source code (identical; +source+ is an alias)
  # - +lang+     fenced language tag, or +nil+ for indented blocks
  # - +children+ always empty; content is plain text, not parsed inline events
  # - Mutable: +html=+
  # - Note: even when +syntax_highlight: true+ is enabled, the handler still
  #   sees the event as +:code_block+. Setting +html=+ overrides the highlighter.
  #
  # #### +:table+
  # - +children+ +:table_head+ and +:table_row+ elements
  # - Use +children_of(:table_row)+ to get body rows; +children_of(:table_head)+
  #   to get the header.
  # - Mutable: +html=+
  #
  # #### +:table_head+
  # - +children+ +:table_row+ elements (the header row)
  # - Mutable: +html=+
  #
  # #### +:table_row+
  # - +text+     concatenated plain text of all cells
  # - +children+ +:table_cell+ elements
  # - Mutable: +html=+
  #
  # #### +:table_cell+
  # - +text+     plain text of the cell
  # - +children+ inline child nodes
  # - Mutable: +html=+
  #
  # #### +:emphasis+
  # - +text+     plain text of content
  # - +children+ inline child nodes
  # - Mutable: +html=+
  #
  # #### +:strong+
  # - +text+     plain text of content
  # - +children+ inline child nodes
  # - Mutable: +html=+
  #
  # #### +:strikethrough+
  # - +text+     plain text of content
  # - +children+ inline child nodes
  # - Mutable: +html=+
  #
  # #### +:link+
  # - +text+     link label plain text
  # - +dest+     URL (mutable via +dest=+)
  # - +title+    tooltip/title attribute, or +nil+ (mutable via +title=+)
  # - +children+ inline child nodes (text, image, strong, …)
  # - Mutable: +dest=+, +title=+, +html=+
  #
  # #### +:image+
  # - +text+     alt text
  # - +dest+     image URL (mutable via +dest=+)
  # - +title+    title attribute, or +nil+ (mutable via +title=+)
  # - +children+ always empty; alt text is plain text, not parsed inline events
  # - Mutable: +dest=+, +title=+, +html=+
  #
  # #### +:footnote_definition+
  # - +text+     plain text of the footnote body
  # - +children+ block child nodes
  # - Mutable: +html=+
  #
  # ### Leaf kinds
  #
  # Leaves are single events with no children. Their handler fires on the
  # event itself.
  #
  # #### +:code+  (inline code)
  # - +text+     the code content, without backtick delimiters
  # - +children+ always empty
  # - Mutable: +html=+
  #
  # #### +:text+
  # - +text+     the text string (after emoji resolution if enabled)
  # - +children+ always empty
  # - Mutable: +html=+
  #
  # #### +:html+  (inline raw HTML, when +suppress_raw_html: false+)
  # - +text+     the raw HTML string
  # - +children+ always empty
  # - Mutable: +html=+
  #
  # #### +:rule+  (thematic break / horizontal rule)
  # - +text+     always an empty string
  # - +children+ always empty
  # - Mutable: +html=+
  #
  # #### +:soft_break+
  # - +text+     always an empty string
  # - +children+ always empty
  # - Mutable: +html=+
  #
  # #### +:hard_break+
  # - +text+     always an empty string
  # - +children+ always empty
  # - Mutable: +html=+
  #
  # #### +:footnote_reference+
  # - +text+     the footnote label
  # - +children+ always empty
  # - Mutable: +html=+
  #
  # ## Mutation precedence
  #
  # 1. {#delete}    suppresses the element entirely (nothing emitted)
  # 2. {#html=}     emits a raw HTML string; skips all other rendering
  # 3. {#markdown=} re-parses the string and splices the events in
  # 4. Field mutations (+dest=+, +level=+, +id=+, +title=+)—modify the
  #    element's normal rendering
  #
  # When +html=+ is set, +markdown=+ and field mutations are ignored.
  # When +markdown=+ is set, field mutations are ignored.
  # {#delete} takes priority over everything.
  #
  # ## Filter interaction
  #
  # Enrichment filters run **before** handlers, so handlers always see:
  # - Emoji shortcodes already resolved (+emoji_shortcodes: true+)
  # - URLs already autolinked (+links: { autolink: true }+)
  # - Heading +id+ already set (+headings: { ids: true }+)
  # - Raw HTML already suppressed (+suppress_raw_html: true+—the default)
  #
  # Post-render filters (+syntax_highlight+, +links: { allowed_hosts: ... }+, etc.) run
  # **after** handlers, so handler-set +dest=+ values are subject to allowlist
  # filtering.
  class Event
    # The element kind.
    # @return [Symbol]
    attr_reader :kind

    # Concatenated plain text of all descendant text nodes.
    # For leaf elements this is the element's own text.
    # @return [String]
    attr_reader :text

    # Raw source code for +:code_block+ elements. Identical to +text+.
    # +nil+ for all other kinds.
    # @return [String, nil]
    attr_reader :source

    # Language tag for +:code_block+ elements (e.g. +"ruby"+, +"javascript"+).
    # +nil+ for indented code blocks and all other element kinds.
    # @return [String, nil]
    attr_reader :lang

    # Byte offsets of this element in the source string, as an exclusive Ruby
    # Range (+start...end+). Use it to slice the original source:
    #
    #   source[event.byte_range]  # → the raw markdown for this element
    #
    # Populated for: all container kinds (+:heading+, +:paragraph+,
    # +:blockquote+, +:code_block+, +:table+, +:list+, +:link+, +:image+, …)
    # and the leaf kinds +:code+ (inline code), +:rule+, +:inline_math+,
    # +:display_math+.
    #
    # +nil+ for +:text+, +:soft_break+, +:hard_break+, and +:footnote_reference+
    # (those events can be split or merged by filters).
    #
    # +:link+ byte ranges are also +nil+ when +links: { autolink: true }+ is
    # enabled—the autolink filter inserts new link events that would shift
    # the queue, so ranges for that kind are suppressed.
    #
    # @return [Range, nil]
    attr_reader :byte_range

    # Nesting depth of this element. Top-level blocks have depth 0;
    # a paragraph inside a blockquote has depth 1.
    # @return [Integer]
    attr_reader :depth

    # Kind of the immediate parent element, or +nil+ when this element
    # is at the document root.
    # @return [Symbol, nil]
    attr_reader :parent_kind

    # Ancestor kinds from immediate parent to root, nearest first.
    # Empty when this element is at the document root.
    # @return [Array<Symbol>]
    attr_reader :ancestor_kinds

    # URL for +:link+ and +:image+ elements. Setting this rewrites the +href+
    # or +src+ in the rendered output. Subject to allowlist filters after handlers run.
    # +nil+ for all other kinds.
    # @return [String, nil]
    attr_accessor :dest

    # Title attribute for +:link+ and +:image+ elements (rendered as the
    # +title+ tooltip). +nil+ when no title was specified or for other kinds.
    # @return [String, nil]
    attr_accessor :title

    # Heading level (1–6) for +:heading+ elements. Setting this changes the
    # rendered tag (e.g. +<h2>+ → +<h3>+). +nil+ for all other kinds.
    # @return [Integer, nil]
    attr_accessor :level

    # HTML +id+ attribute for +:heading+ elements. Set automatically when
    # +headings: { ids: true }+ is enabled; +nil+ otherwise. Setting this overrides
    # the generated slug. +nil+ for all other kinds.
    # @return [String, nil]
    attr_accessor :id

    # Replace this element's entire output with a raw HTML string.
    # When set, all other mutations and the element's default rendering
    # are ignored. The string is emitted verbatim—no escaping applied.
    # Takes priority over {#markdown=}.
    # @return [String, nil]
    attr_accessor :html

    # Replace this element's output by re-rendering a markdown string.
    # The replacement is parsed with the same options as the main document
    # (emoji expansion, heading IDs, raw HTML suppression, etc.) and is
    # subject to post-render filters (+syntax_highlight+, allowlists, etc.).
    # Handlers do NOT fire on elements within the replacement.
    # {#html=} takes priority when both are set; {#delete} takes priority
    # over everything.
    # Has no effect when called inside a {Inkmark#walk} block (walk produces
    # no output).
    # @return [String, nil]
    attr_accessor :markdown

    def initialize(data) # :nodoc:
      @kind = data[:kind].to_sym
      @text = data[:text] || ""
      @source = @text
      @lang = data[:lang]
      @dest = data[:dest]
      @title = data[:title]
      @level = data[:level]
      @id = data[:id]
      @byte_range = data[:byte_range]
      @depth = data[:depth] || 0
      @parent_kind = data[:parent_kind]&.to_sym
      @ancestor_kinds = (data[:ancestor_kinds] || []).map(&:to_sym)
      @children_data = data[:children] || []
      @html = nil
      @markdown = nil
      @deleted = false
    end

    # Direct child elements of this node (lazy, cached).
    # For leaf elements (text, code, rule, soft_break, hard_break,
    # footnote_reference, html) this is always an empty array.
    # @return [Array<Event>]
    def children
      @children ||= @children_data.map { |d| Event.new(d) }
    end

    # Direct children filtered by kind.
    #
    # @example Get body rows from a table
    #   md.on(:table) { |t| rows = t.children_of(:table_row) }
    # @example Get cells from a row
    #   row.children_of(:table_cell).map(&:text)
    #
    # @param kind [Symbol] element kind to select (e.g. +:table_row+)
    # @return [Array<Event>]
    def children_of(kind)
      children.select { |c| c.kind == kind }
    end

    # Suppress this element—nothing is emitted to the HTML output.
    # Takes precedence over {#html=} and all other mutations.
    # @return [void]
    def delete
      @deleted = true
    end

    # Returns +true+ if {#delete} was called on this element.
    # @return [Boolean]
    def deleted?
      @deleted
    end
  end
end
