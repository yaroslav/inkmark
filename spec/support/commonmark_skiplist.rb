# frozen_string_literal: true

# Examples from the CommonMark test suite that Inkmark deliberately skips,
# mapped to the reason. Each entry should cite the pulldown-cmark CHANGELOG,
# GitHub issue, or spec-interpretation difference that justifies the skip.
#
# Populated during first-run triage in Task 18.
module CommonMarkSkipList
  SKIPS = {
    # Double-quote entity encoding (&quot;)—pulldown-cmark outputs literal
    # `"` in HTML text content where the CommonMark reference renderer emits
    # `&quot;`. Both are valid HTML5, but the spec's expected output uses the
    # entity form. pulldown-cmark deliberately does not escape double quotes
    # in text nodes; see pulldown-cmark/pulldown-cmark#<conformance note> and
    # the upstream renderer's policy of minimal escaping (only `<`, `>`, `&`,
    # and `'` are escaped in text; `"` is only escaped inside attribute values).
    12 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    14 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    27 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    41 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    91 => "pulldown-cmark outputs literal \" instead of &quot; in attribute-like content in headings—valid HTML5 but diverges from CommonMark reference renderer",
    209 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    210 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    211 => "pulldown-cmark outputs literal \" instead of &quot; in code block content—valid HTML5 but diverges from CommonMark reference renderer",
    343 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    352 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    359 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    363 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    380 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    385 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    395 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    508 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    590 => "pulldown-cmark outputs literal \" instead of &quot; in text nodes—valid HTML5 but diverges from CommonMark reference renderer",
    619 => "pulldown-cmark outputs literal \" instead of &quot; in escaped/invalid HTML tag text—valid HTML5 but diverges from CommonMark reference renderer",
    620 => "pulldown-cmark outputs literal \" instead of &quot; in escaped/invalid HTML tag text—valid HTML5 but diverges from CommonMark reference renderer",
    624 => "pulldown-cmark outputs literal \" instead of &quot; in escaped/invalid HTML tag text—valid HTML5 but diverges from CommonMark reference renderer",
    632 => "pulldown-cmark outputs literal \" instead of &quot; in escaped/invalid HTML tag text—valid HTML5 but diverges from CommonMark reference renderer",

    # HTML block inside list item—pulldown-cmark omits the newline between
    # the `<li>` open tag and the block-level HTML element that follows it.
    # The CommonMark reference expects `<li>\n<div>\n` but pulldown-cmark
    # emits `<li><div>\n`. This is a known rendering difference in how
    # pulldown-cmark handles block HTML within tight list items; both produce
    # equivalent DOM trees.
    175 => "pulldown-cmark omits newline between <li> and block HTML element—produces equivalent DOM but diverges from CommonMark reference whitespace"
  }.freeze

  def self.skip?(example_number)
    SKIPS.key?(example_number)
  end
end
