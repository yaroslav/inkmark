#![forbid(unsafe_code)]

use magnus::{function, prelude::*, Error, Ruby};

mod autolink;
mod chunks_by_heading;
mod chunks_by_size;
mod document;
mod emoji;
mod handler;
mod heading;
mod highlight;
mod image;
mod link;
mod options;
mod plain_text;
mod scheme_filter;
mod stats;
mod tag_filter;
mod toc;
mod truncate;
mod url_match;

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let inkmark = ruby.define_class("Inkmark", ruby.class_object())?;
    inkmark.define_singleton_method("_native_to_html", function!(document::native_to_html, 2))?;
    inkmark.define_singleton_method("_native_to_markdown", function!(document::native_to_markdown, 2))?;
    inkmark.define_singleton_method("_native_to_plain_text", function!(document::native_to_plain_text, 2))?;
    inkmark.define_singleton_method(
        "_native_chunks_by_heading",
        function!(chunks_by_heading::native_chunks_by_heading, 2),
    )?;
    inkmark.define_singleton_method(
        "_native_chunks_by_size",
        function!(chunks_by_size::native_chunks_by_size, 2),
    )?;
    inkmark.define_singleton_method(
        "_native_truncate_markdown",
        function!(truncate::native_truncate_markdown, 3),
    )?;
    inkmark.define_singleton_method(
        "_native_render_full",
        function!(document::native_render_full, 2),
    )?;
    inkmark.define_singleton_method("_syntax_css", function!(highlight::syntax_css, 1))?;
    inkmark.define_singleton_method("_syntax_themes", function!(highlight::syntax_themes, 0))?;
    inkmark.define_singleton_method("_native_walk", function!(handler::native_walk, 3))?;
    inkmark.define_singleton_method(
        "_native_render_with_handlers",
        function!(handler::native_render_with_handlers, 3),
    )?;
    Ok(())
}
