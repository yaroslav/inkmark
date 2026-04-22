# frozen_string_literal: true

begin
  ruby_version = RUBY_VERSION[/\d+\.\d+/]
  require_relative "#{ruby_version}/inkmark"
rescue LoadError
  require_relative "inkmark"
end
