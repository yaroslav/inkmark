# dotenv [![Gem Version](https://badge.fury.io/rb/dotenv.svg)](https://badge.fury.io/rb/dotenv)

Shim to load environment variables from `.env` into `ENV` in *development*.

Storing [configuration in the environment](http://12factor.net/config) is one of the tenets of a [twelve-factor app](http://12factor.net). Anything that is likely to change between deployment environments–such as resource handles for databases or credentials for external services–should be extracted from the code into environment variables.

But it is not always practical to set environment variables on development machines or continuous integration servers where multiple projects are run. dotenv loads variables from a `.env` file into `ENV` when the environment is bootstrapped.

## Installation

Add this line to the top of your application's Gemfile and run `bundle install`:

```ruby
gem 'dotenv', groups: [:development, :test]
```

## Usage

Add your application configuration to your `.env` file in the root of your project:

```shell
S3_BUCKET=YOURS3BUCKET
SECRET_KEY=YOURSECRETKEYGOESHERE
```

Whenever your application loads, these variables will be available in `ENV`:

```ruby
config.fog_directory = ENV['S3_BUCKET']
```

See the [API Docs](https://rubydoc.info/github/bkeepers/dotenv/main) for more.

### Rails

Dotenv will automatically load when your Rails app boots. See [Customizing Rails](#customizing-rails) to change which files are loaded and when.

### Sinatra / Ruby

Load Dotenv as early as possible in your application bootstrap process:

```ruby
require 'dotenv/load'

# or
require 'dotenv'
Dotenv.load
```

By default, `load` will look for a file called `.env` in the current working directory.
Pass in multiple files and they will be loaded in order.
The first value set for a variable will win.
Existing environment variables will not be overwritten unless you set `overwrite: true`.

```ruby
require 'dotenv'
Dotenv.load('file1.env', 'file2.env')
```

### Autorestore in tests

Since 3.0, dotenv in a Rails app will automatically restore `ENV` after each test. This means you can modify `ENV` in your tests without fear of leaking state to other tests. It works with both `ActiveSupport::TestCase` and `Rspec`.

To disable this behavior, set `config.dotenv.autorestore = false` in `config/application.rb` or `config/environments/test.rb`. It is disabled by default if your app uses [climate_control](https://github.com/thoughtbot/climate_control) or [ice_age](https://github.com/dpep/ice_age_rb).

To use this behavior outside of a Rails app, just `require "dotenv/autorestore"` in your test suite.

See [`Dotenv.save`](https://rubydoc.info/github/bkeepers/dotenv/main/Dotenv:save), [Dotenv.restore](https://rubydoc.info/github/bkeepers/dotenv/main/Dotenv:restore), and [`Dotenv.modify(hash) { ... }`](https://rubydoc.info/github/bkeepers/dotenv/main/Dotenv:modify) for manual usage.

### Rake

To ensure `.env` is loaded in rake, load the tasks:

```ruby
require 'dotenv/tasks'

task mytask: :dotenv do
  # things that require .env
end
```

### CLI

You can use the `dotenv` executable load `.env` before launching your application:

```console
$ dotenv ./script.rb
```

The `dotenv` executable also accepts the flag `-f`. Its value should be a comma-separated list of configuration files, in the order of the most important to the least important. All of the files must exist. There _must_ be a space between the flag and its value.

```console
$ dotenv -f ".env.local,.env" ./script.rb
```

The `dotenv` executable can optionally ignore missing files with the `-i` or `--ignore` flag. For example, if the `.env.local` file does not exist, the following will ignore the missing file and only load the `.env` file.

```console
$ dotenv -i -f ".env.local,.env" ./script.rb
```
