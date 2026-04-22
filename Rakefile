# frozen_string_literal: true

require "bundler/gem_tasks"
require "rspec/core/rake_task"

RSpec::Core::RakeTask.new(:spec)

require "standard/rake"

require "rb_sys/extensiontask"

task build: :compile

GEMSPEC = Gem::Specification.load("inkmark.gemspec")

RUBY_VERSION_FILE = File.expand_path("lib/inkmark/version.rb", __dir__)
CARGO_TOML = File.expand_path("ext/inkmark/Cargo.toml", __dir__)

desc "Sync ext/inkmark/Cargo.toml [package] version from lib/inkmark/version.rb"
task :sync_version do
  ruby_version = File.read(RUBY_VERSION_FILE)[/VERSION\s*=\s*["']([^"']+)["']/, 1]
  raise "Could not read VERSION from #{RUBY_VERSION_FILE}" if ruby_version.nil?

  content = File.read(CARGO_TOML)
  cargo_version = content[/^version\s*=\s*"([^"]+)"/, 1]
  raise "Could not find [package] version in #{CARGO_TOML}" if cargo_version.nil?

  if cargo_version == ruby_version
    # Idempotent no-op — don't dirty the working tree when versions already match.
    next
  end

  updated = content.sub(/^version\s*=\s*"[^"]+"/, %(version = "#{ruby_version}"))
  File.write(CARGO_TOML, updated)
  puts "inkmark: synced ext/inkmark/Cargo.toml version #{cargo_version} -> #{ruby_version}"
end

# Declared BEFORE RbSys::ExtensionTask.new so sync_version is the first
# prerequisite inserted into :compile. rb-sys's extension task appends its
# platform-specific compile chain after this, giving us the desired order:
# sync_version runs, then cargo, then the rest of rb-sys's wrappers.
task compile: :sync_version

RbSys::ExtensionTask.new("inkmark", GEMSPEC) do |ext|
  ext.lib_dir = "lib/inkmark"
end

desc "Run Rust unit tests (cargo test)"
task :cargo_test do
  sh "cargo test --manifest-path #{CARGO_TOML}"
end

task default: %i[compile cargo_test spec standard]

desc "Run the benchmark suite (requires the :benchmark Gemfile group)"
task :benchmark do
  ruby "benchmarks/run.rb"
end
