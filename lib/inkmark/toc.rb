# frozen_string_literal: true

class Inkmark
  # A rendered table of contents, carrying both Markdown and HTML
  # renderings. Returned by {Inkmark#toc} when the +toc: true+,
  # +statistics: true+, or +extract: { headings: true }+ option is
  # set; +nil+ otherwise.
  #
  # @example
  #   g = Inkmark.new(source, options: { toc: true })
  #   g.toc.to_markdown  # => "- [Intro](#intro)\n  - [Goals](#goals)\n"
  #   g.toc.to_html      # => "<ul>\n<li><a href=\"#intro\">...</a>..."
  #   g.toc.to_s         # => same as to_markdown (String coercion)
  #   puts g.toc         # prints the markdown form
  #
  # Immutable value object: the instance is frozen at construction, and
  # +==+ / +eql?+ / +hash+ implement value-equality over the two fields.
  class Toc
    attr_reader :markdown, :html
    alias_method :to_markdown, :markdown
    alias_method :to_html, :html

    def initialize(markdown:, html:)
      @markdown = markdown
      @html = html
      freeze
    end

    def to_s = @markdown

    def ==(other)
      other.is_a?(Toc) && other.markdown == @markdown && other.html == @html
    end
    alias_method :eql?, :==

    def hash
      [self.class, @markdown, @html].hash
    end
  end
end
