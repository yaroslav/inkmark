# [![Faraday](./docs/_media/home-logo.svg)][website]

[![Gem Version](https://badge.fury.io/rb/faraday.svg)](https://rubygems.org/gems/faraday)
[![GitHub Actions CI](https://github.com/lostisland/faraday/workflows/CI/badge.svg)](https://github.com/lostisland/faraday/actions?query=workflow%3ACI)
[![GitHub Discussions](https://img.shields.io/github/discussions/lostisland/faraday?logo=github)](https://github.com/lostisland/faraday/discussions)

Faraday is an HTTP client library abstraction layer that provides a common interface over many
adapters (such as Net::HTTP) and embraces the concept of Rack middleware when processing the request/response cycle.
Take a look at [Awesome Faraday][awesome] for a list of available adapters and middleware.

## Why use Faraday?

Faraday gives you the power of Rack middleware for manipulating HTTP requests and responses,
making it easier to build sophisticated API clients or web service libraries that abstract away
the details of how HTTP requests are made.
