// `deny` rather than `forbid`: the single `unsafe` block in `init` is
// allowed explicitly; everything else stays unsafe-free.
#![deny(unsafe_code)]

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
#[allow(unsafe_code)]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // Declare every method defined below Ractor-safe. Ruby marks the methods an
    // extension defines while `Init_*` runs as unsafe unless this flag is set,
    // and calling such a method from a non-main Ractor raises
    // `Ractor::UnsafeError`. The extension keeps no Ruby `VALUE`s in
    // process-global state: the `OnceLock` caches in `highlight` hold plain
    // syntect data (`Sync`, enforced by the compiler) and the `sym_id!`
    // statics in `options` hold interned symbol ids, both safe to share across
    // Ractors. This must run before `define_class` so the flag covers every
    // method registered below.
    //
    // SAFETY: `rb_ext_ractor_safe` takes no pointers and only flips a flag on
    // the VM's extension-loading state. Its one precondition is being called
    // while `Init_*` runs, which is exactly where `#[magnus::init]` invokes
    // this function.
    unsafe { rb_sys::rb_ext_ractor_safe(true) };

    let inkmark = ruby.define_class("Inkmark", ruby.class_object())?;
    inkmark.define_singleton_method("_native_to_html", function!(document::native_to_html, 2))?;
    inkmark.define_singleton_method(
        "_native_to_markdown",
        function!(document::native_to_markdown, 2),
    )?;
    inkmark.define_singleton_method(
        "_native_to_plain_text",
        function!(document::native_to_plain_text, 2),
    )?;
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
