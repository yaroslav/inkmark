# frozen_string_literal: true

require_relative "lib/inkmark/version"

Gem::Specification.new do |spec|
  spec.name = "inkmark"
  spec.version = Inkmark::VERSION
  spec.authors = ["Yaroslav Markin"]
  spec.email = ["yaroslav@markin.net"]

  spec.summary = "Very fast, feature-packed, AI-first markdown gem for Ruby."
  spec.description = "A very fast, feature-packed, AI-first markdown (CommonMark/GFM) gem for Ruby, based on pulldown-cmark (Rust)."
  spec.homepage = "https://github.com/yaroslav/inkmark"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.3.0"
  spec.metadata["homepage_uri"] = spec.homepage
  spec.metadata["source_code_uri"] = "https://github.com/yaroslav/inkmark"
  spec.metadata["changelog_uri"] = "https://github.com/yaroslav/inkmark/blob/main/CHANGELOG.md"
  spec.metadata["bug_tracker_uri"] = "https://github.com/yaroslav/inkmark/issues"
  spec.metadata["documentation_uri"] = "https://rubydoc.info/gems/inkmark"

  spec.files = Dir["lib/**/*.rb"] +
    Dir["sig/**/*.rbs"] +
    Dir["ext/**/*.{rb,rs,toml}"] +
    %w[README.md CHANGELOG.md LICENSE.txt NOTICE Cargo.toml Cargo.lock]

  spec.bindir = "exe"
  spec.executables = spec.files.grep(%r{\Aexe/}) { |f| File.basename(f) }
  spec.require_paths = ["lib"]
  spec.extensions = ["ext/inkmark/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9.126"
  spec.add_development_dependency "rake", "~> 13.0"
  spec.add_development_dependency "irb"
  spec.add_development_dependency "rbs", "~> 3.9"
  spec.add_development_dependency "yard", "~> 0.9"
  spec.add_development_dependency "standard", "~> 1.3"
  spec.add_development_dependency "rspec", "~> 3.0"
  spec.add_development_dependency "lefthook", "~> 2.1.5"
  spec.add_development_dependency "rake-compiler"
end
