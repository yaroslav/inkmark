use magnus::{prelude::*, Error, RArray, RHash, Ruby, Value};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType,
    MetadataBlockKind, Parser, Tag, TagEnd,
};
use std::collections::{HashMap, VecDeque};
use std::ops::Range;

use crate::document::{apply_post_handler_filters, apply_pre_handler_filters};
use crate::options::{build_options, Flags};

/// Per-kind data needed to reconstruct Start events on serialization —
/// only fields that are NOT exposed as mutable handler targets.
enum NodeExtra {
    Heading {
        classes: Vec<CowStr<'static>>,
        attrs: Vec<(CowStr<'static>, Option<CowStr<'static>>)>,
    },
    Link {
        link_type: LinkType,
        link_id: CowStr<'static>,
    },
    Image {
        link_type: LinkType,
        link_id: CowStr<'static>,
    },
    List,
    CodeBlock {
        fenced: bool,
    },
    Table {
        alignments: Vec<Alignment>,
    },
    BlockQuote {
        kind: Option<BlockQuoteKind>,
    },
    MetadataBlock {
        kind: MetadataBlockKind,
    },
    FootnoteDefinition {
        label: CowStr<'static>,
    },
    LeafEvent(Event<'static>),
    None,
}

pub(crate) struct Node {
    kind: &'static str,
    is_container: bool,
    text: String,
    depth: usize,
    children: Vec<Node>,
    parent_kind: Option<&'static str>,
    byte_range: Option<Range<usize>>,
    // Fields exposed to / mutated by handlers.
    level: Option<u8>,
    lang: Option<String>,
    dest: Option<String>,
    title: Option<String>,
    id: Option<String>,
    label: Option<String>,
    extra: NodeExtra,
    // Mutations written back after handler dispatch.
    replacement_html: Option<String>,
    replacement_markdown: Option<String>,
    new_dest: Option<String>,
    new_title: Option<String>,
    new_level: Option<u8>,
    new_id: Option<String>,
    deleted: bool,
}

fn own_str(s: &str) -> CowStr<'static> {
    CowStr::Boxed(s.to_string().into_boxed_str())
}

fn level_from_u8(n: u8) -> HeadingLevel {
    match n {
        1 => HeadingLevel::H1,
        2 => HeadingLevel::H2,
        3 => HeadingLevel::H3,
        4 => HeadingLevel::H4,
        5 => HeadingLevel::H5,
        _ => HeadingLevel::H6,
    }
}

fn kind_for_tag(tag: &Tag<'_>) -> &'static str {
    match tag {
        Tag::Paragraph => "paragraph",
        Tag::Heading { .. } => "heading",
        Tag::BlockQuote(_) => "blockquote",
        Tag::CodeBlock(_) => "code_block",
        Tag::HtmlBlock => "html_block",
        Tag::List(None) => "list",
        Tag::List(Some(_)) => "ordered_list",
        Tag::Item => "list_item",
        Tag::FootnoteDefinition(_) => "footnote_definition",
        Tag::Table(_) => "table",
        Tag::TableHead => "table_head",
        Tag::TableRow => "table_row",
        Tag::TableCell => "table_cell",
        Tag::Emphasis => "emphasis",
        Tag::Strong => "strong",
        Tag::Strikethrough => "strikethrough",
        Tag::Link { .. } => "link",
        Tag::Image { .. } => "image",
        Tag::DefinitionList => "definition_list",
        Tag::DefinitionListTitle => "definition_list_title",
        Tag::DefinitionListDefinition => "definition_list_definition",
        Tag::Superscript => "superscript",
        Tag::Subscript => "subscript",
        Tag::MetadataBlock(_) => "metadata_block",
    }
}

type RangeMap = HashMap<&'static str, VecDeque<Range<usize>>>;

/// Parse `source` with the offset iterator to collect byte ranges per element
/// kind, keyed by kind string. Ranges are ordered by source position so that
/// `build_tree` can pop them in document order.
///
/// `has_autolink`: when true, "link" ranges are excluded. autolink inserts
/// new Start(Link) events inline, which would shift the queue and assign
/// ranges from explicit links to the wrong nodes.
fn collect_byte_ranges(
    source: &str,
    cm_opts: pulldown_cmark::Options,
    has_autolink: bool,
) -> RangeMap {
    let mut map: RangeMap = HashMap::new();
    for (event, range) in Parser::new_ext(source, cm_opts).into_offset_iter() {
        let kind: Option<&'static str> = match &event {
            Event::Start(tag) => match tag {
                // autolink inserts extra Start(Link) events—skip to avoid
                // corrupting the per-kind queue ordering for explicit links.
                Tag::Link { .. } if has_autolink => None,
                _ => Some(kind_for_tag(tag)),
            },
            // Inline code spans: autolink never splits these.
            Event::Code(_) => Some("code"),
            Event::Rule => Some("rule"),
            Event::InlineMath(_) => Some("inline_math"),
            Event::DisplayMath(_) => Some("display_math"),
            // Text events can be split by autolink/emoji—skip.
            _ => None,
        };
        if let Some(k) = kind {
            map.entry(k).or_default().push_back(range);
        }
    }
    map
}

fn node_from_start(
    tag: Tag<'_>,
    depth: usize,
    parent_kind: Option<&'static str>,
    byte_range: Option<Range<usize>>,
) -> Node {
    let kind = kind_for_tag(&tag);
    let (level, lang, dest, title, id, label, extra) = match tag {
        Tag::Heading {
            level,
            id,
            classes,
            attrs,
        } => {
            let lv = crate::toc::level_to_u8(level);
            let id_s = id.as_deref().map(str::to_string);
            let extra = NodeExtra::Heading {
                classes: classes.into_iter().map(|s| s.into_static()).collect(),
                attrs: attrs
                    .into_iter()
                    .map(|(k, v)| (k.into_static(), v.map(|s| s.into_static())))
                    .collect(),
            };
            (Some(lv), None, None, None, id_s, None, extra)
        }
        Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        } => {
            let d = dest_url.as_ref().to_string();
            let t = title.as_ref().to_string();
            let extra = NodeExtra::Link {
                link_type,
                link_id: id.into_static(),
            };
            (None, None, Some(d), Some(t), None, None, extra)
        }
        Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        } => {
            let d = dest_url.as_ref().to_string();
            let t = title.as_ref().to_string();
            let extra = NodeExtra::Image {
                link_type,
                link_id: id.into_static(),
            };
            (None, None, Some(d), Some(t), None, None, extra)
        }
        Tag::CodeBlock(ref cbk) => {
            let (lang_s, fenced) = match cbk {
                CodeBlockKind::Fenced(lang) => (lang.as_ref().to_string(), true),
                CodeBlockKind::Indented => (String::new(), false),
            };
            (
                None,
                Some(lang_s),
                None,
                None,
                None,
                None,
                NodeExtra::CodeBlock { fenced },
            )
        }
        Tag::List(_start) => (None, None, None, None, None, None, NodeExtra::List),
        Tag::FootnoteDefinition(lbl) => {
            let label_s = lbl.as_ref().to_string();
            (
                None,
                None,
                None,
                None,
                None,
                Some(label_s),
                NodeExtra::FootnoteDefinition {
                    label: lbl.into_static(),
                },
            )
        }
        Tag::Table(alignments) => (
            None,
            None,
            None,
            None,
            None,
            None,
            NodeExtra::Table { alignments },
        ),
        Tag::BlockQuote(kind) => (
            None,
            None,
            None,
            None,
            None,
            None,
            NodeExtra::BlockQuote { kind },
        ),
        Tag::MetadataBlock(kind) => (
            None,
            None,
            None,
            None,
            None,
            None,
            NodeExtra::MetadataBlock { kind },
        ),
        _ => (None, None, None, None, None, None, NodeExtra::None),
    };
    Node {
        kind,
        is_container: true,
        text: String::new(),
        depth,
        children: Vec::new(),
        parent_kind,
        byte_range,
        level,
        lang,
        dest,
        title,
        id,
        label,
        extra,
        replacement_html: None,
        replacement_markdown: None,
        new_dest: None,
        new_title: None,
        new_level: None,
        new_id: None,
        deleted: false,
    }
}

fn node_from_leaf(
    event: Event<'_>,
    depth: usize,
    parent_kind: Option<&'static str>,
    byte_range: Option<Range<usize>>,
) -> Node {
    let (kind, text, label) = match &event {
        Event::Text(s) => ("text", s.as_ref().to_string(), None),
        Event::Code(s) => ("code", s.as_ref().to_string(), None),
        Event::Html(s) | Event::InlineHtml(s) => ("html", s.as_ref().to_string(), None),
        Event::SoftBreak => ("soft_break", String::new(), None),
        Event::HardBreak => ("hard_break", String::new(), None),
        Event::Rule => ("rule", String::new(), None),
        Event::FootnoteReference(s) => {
            let label = s.as_ref().to_string();
            ("footnote_reference", label.clone(), Some(label))
        }
        Event::InlineMath(s) => ("inline_math", s.as_ref().to_string(), None),
        Event::DisplayMath(s) => ("display_math", s.as_ref().to_string(), None),
        Event::TaskListMarker(_) => ("task_list_marker", String::new(), None),
        _ => ("unknown", String::new(), None),
    };
    Node {
        kind,
        is_container: false,
        text,
        depth,
        children: Vec::new(),
        parent_kind,
        byte_range,
        level: None,
        lang: None,
        dest: None,
        title: None,
        id: None,
        label,
        extra: NodeExtra::LeafEvent(event.into_static()),
        replacement_html: None,
        replacement_markdown: None,
        new_dest: None,
        new_title: None,
        new_level: None,
        new_id: None,
        deleted: false,
    }
}

fn collect_text(children: &[Node]) -> String {
    children.iter().map(|c| c.text.as_str()).collect()
}

/// Kind string for leaves that have stable byte ranges (not affected by
/// autolink or emoji splitting). Used to look up ranges in the range map.
fn leaf_range_kind(event: &Event<'_>) -> Option<&'static str> {
    match event {
        Event::Code(_) => Some("code"),
        Event::Rule => Some("rule"),
        Event::InlineMath(_) => Some("inline_math"),
        Event::DisplayMath(_) => Some("display_math"),
        _ => None,
    }
}

pub fn build_tree(events: Vec<Event<'_>>, ranges: &mut RangeMap) -> Vec<Node> {
    let mut stack: Vec<Node> = Vec::new();
    let mut roots: Vec<Node> = Vec::new();

    for event in events {
        match event {
            Event::Start(tag) => {
                let depth = stack.len();
                let parent_kind = stack.last().map(|n| n.kind);
                let kind = kind_for_tag(&tag);
                let byte_range = ranges.get_mut(kind).and_then(|q| q.pop_front());
                stack.push(node_from_start(tag, depth, parent_kind, byte_range));
            }
            Event::End(_) => {
                if let Some(mut node) = stack.pop() {
                    node.text = collect_text(&node.children);
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        roots.push(node);
                    }
                }
            }
            leaf => {
                let depth = stack.len();
                let parent_kind = stack.last().map(|n| n.kind);
                let byte_range = leaf_range_kind(&leaf)
                    .and_then(|k| ranges.get_mut(k).and_then(|q| q.pop_front()));
                let leaf_node = node_from_leaf(leaf, depth, parent_kind, byte_range);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(leaf_node);
                } else {
                    roots.push(leaf_node);
                }
            }
        }
    }

    roots
}

fn tagend_for(node: &Node) -> TagEnd {
    match node.kind {
        "paragraph" => TagEnd::Paragraph,
        "heading" => {
            let lv = node.new_level.unwrap_or(node.level.unwrap_or(1));
            TagEnd::Heading(level_from_u8(lv))
        }
        "blockquote" => {
            let kind = if let NodeExtra::BlockQuote { kind } = &node.extra {
                *kind
            } else {
                None
            };
            TagEnd::BlockQuote(kind)
        }
        "list" => TagEnd::List(false),
        "ordered_list" => TagEnd::List(true),
        "list_item" => TagEnd::Item,
        "code_block" => TagEnd::CodeBlock,
        "html_block" => TagEnd::HtmlBlock,
        "table" => TagEnd::Table,
        "table_head" => TagEnd::TableHead,
        "table_row" => TagEnd::TableRow,
        "table_cell" => TagEnd::TableCell,
        "emphasis" => TagEnd::Emphasis,
        "strong" => TagEnd::Strong,
        "strikethrough" => TagEnd::Strikethrough,
        "link" => TagEnd::Link,
        "image" => TagEnd::Image,
        "footnote_definition" => TagEnd::FootnoteDefinition,
        "definition_list" => TagEnd::DefinitionList,
        "definition_list_title" => TagEnd::DefinitionListTitle,
        "definition_list_definition" => TagEnd::DefinitionListDefinition,
        "superscript" => TagEnd::Superscript,
        "subscript" => TagEnd::Subscript,
        "metadata_block" => {
            let kind = if let NodeExtra::MetadataBlock { kind } = &node.extra {
                *kind
            } else {
                MetadataBlockKind::YamlStyle
            };
            TagEnd::MetadataBlock(kind)
        }
        _ => TagEnd::Paragraph,
    }
}

fn start_event_for(node: &Node) -> Event<'static> {
    match node.kind {
        "paragraph" => Event::Start(Tag::Paragraph),
        "heading" => {
            let level = level_from_u8(node.new_level.unwrap_or(node.level.unwrap_or(1)));
            let id = node.new_id.as_deref().or(node.id.as_deref()).map(own_str);
            let (classes, attrs) = if let NodeExtra::Heading { classes, attrs } = &node.extra {
                (classes.clone(), attrs.clone())
            } else {
                (vec![], vec![])
            };
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            })
        }
        "blockquote" => {
            let kind = if let NodeExtra::BlockQuote { kind } = &node.extra {
                *kind
            } else {
                None
            };
            Event::Start(Tag::BlockQuote(kind))
        }
        "list" => Event::Start(Tag::List(None)),
        "ordered_list" => Event::Start(Tag::List(Some(1))),
        "list_item" => Event::Start(Tag::Item),
        "code_block" => {
            let lang = node.lang.as_deref().unwrap_or("");
            let fenced = matches!(&node.extra, NodeExtra::CodeBlock { fenced: true });
            if fenced {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(own_str(lang))))
            } else {
                Event::Start(Tag::CodeBlock(CodeBlockKind::Indented))
            }
        }
        "html_block" => Event::Start(Tag::HtmlBlock),
        "table" => {
            let alignments = if let NodeExtra::Table { alignments } = &node.extra {
                alignments.clone()
            } else {
                vec![]
            };
            Event::Start(Tag::Table(alignments))
        }
        "table_head" => Event::Start(Tag::TableHead),
        "table_row" => Event::Start(Tag::TableRow),
        "table_cell" => Event::Start(Tag::TableCell),
        "emphasis" => Event::Start(Tag::Emphasis),
        "strong" => Event::Start(Tag::Strong),
        "strikethrough" => Event::Start(Tag::Strikethrough),
        "link" => {
            let dest = own_str(
                node.new_dest
                    .as_deref()
                    .unwrap_or(node.dest.as_deref().unwrap_or("")),
            );
            let title = own_str(
                node.new_title
                    .as_deref()
                    .unwrap_or(node.title.as_deref().unwrap_or("")),
            );
            let (link_type, link_id) = if let NodeExtra::Link { link_type, link_id } = &node.extra {
                (*link_type, link_id.clone())
            } else {
                (LinkType::Inline, own_str(""))
            };
            Event::Start(Tag::Link {
                link_type,
                dest_url: dest,
                title,
                id: link_id,
            })
        }
        "image" => {
            let dest = own_str(
                node.new_dest
                    .as_deref()
                    .unwrap_or(node.dest.as_deref().unwrap_or("")),
            );
            let title = own_str(
                node.new_title
                    .as_deref()
                    .unwrap_or(node.title.as_deref().unwrap_or("")),
            );
            let (link_type, link_id) = if let NodeExtra::Image { link_type, link_id } = &node.extra
            {
                (*link_type, link_id.clone())
            } else {
                (LinkType::Inline, own_str(""))
            };
            Event::Start(Tag::Image {
                link_type,
                dest_url: dest,
                title,
                id: link_id,
            })
        }
        "footnote_definition" => {
            let label = if let NodeExtra::FootnoteDefinition { label } = &node.extra {
                label.clone()
            } else {
                own_str("")
            };
            Event::Start(Tag::FootnoteDefinition(label))
        }
        "definition_list" => Event::Start(Tag::DefinitionList),
        "definition_list_title" => Event::Start(Tag::DefinitionListTitle),
        "definition_list_definition" => Event::Start(Tag::DefinitionListDefinition),
        "superscript" => Event::Start(Tag::Superscript),
        "subscript" => Event::Start(Tag::Subscript),
        "metadata_block" => {
            let kind = if let NodeExtra::MetadataBlock { kind } = &node.extra {
                *kind
            } else {
                MetadataBlockKind::YamlStyle
            };
            Event::Start(Tag::MetadataBlock(kind))
        }
        _ => Event::Start(Tag::Paragraph),
    }
}

pub fn tree_to_events(
    nodes: Vec<Node>,
    cm_opts: pulldown_cmark::Options,
    flags: &Flags,
) -> Vec<Event<'static>> {
    let mut out = Vec::new();
    serialize_nodes(nodes, &mut out, cm_opts, flags);
    out
}

fn serialize_nodes(
    nodes: Vec<Node>,
    out: &mut Vec<Event<'static>>,
    cm_opts: pulldown_cmark::Options,
    flags: &Flags,
) {
    for node in nodes {
        serialize_node(node, out, cm_opts, flags);
    }
}

fn serialize_node(
    node: Node,
    out: &mut Vec<Event<'static>>,
    cm_opts: pulldown_cmark::Options,
    flags: &Flags,
) {
    if node.deleted {
        return;
    }
    // html= takes priority over markdown=; both override default rendering.
    if let Some(html) = node.replacement_html {
        out.push(Event::Html(CowStr::Boxed(html.into_boxed_str())));
        return;
    }
    if let Some(md_src) = node.replacement_markdown {
        // Re-parse the replacement markdown and apply the same enrichment
        // filters (emoji, heading_ids, suppress_raw_html) so the fragment
        // feels native. Handler dispatch is skipped—only the top-level
        // document's handlers fire. Post-handler filters (syntax_highlight,
        // allowlists) apply automatically since they run on the full stream
        // after tree_to_events returns.
        let sub_events: Vec<Event<'static>> = Parser::new_ext(&md_src, cm_opts)
            .map(|e| e.into_static())
            .collect();
        let filtered = apply_pre_handler_filters(sub_events, flags);
        out.extend(filtered);
        return;
    }
    if node.is_container {
        // Compute end tag before consuming node.children.
        let end = tagend_for(&node);
        out.push(start_event_for(&node));
        serialize_nodes(node.children, out, cm_opts, flags);
        out.push(Event::End(end));
    } else if let NodeExtra::LeafEvent(ev) = node.extra {
        out.push(ev);
    }
}

fn get_event_class(ruby: &Ruby) -> Result<magnus::RClass, Error> {
    let inkmark: magnus::RClass = ruby.class_object().const_get("Inkmark")?;
    inkmark.const_get("Event")
}

fn node_to_ruby_hash(node: &Node, ruby: &Ruby) -> Result<RHash, Error> {
    let hash = ruby.hash_new();
    hash.aset(ruby.to_symbol("kind"), ruby.str_new(node.kind))?;
    hash.aset(ruby.to_symbol("text"), ruby.str_new(&node.text))?;
    hash.aset(ruby.to_symbol("depth"), node.depth as i64)?;
    set_optional_str(ruby, &hash, "parent_kind", node.parent_kind)?;

    let ancestors = ruby.ary_new();
    if let Some(pk) = node.parent_kind {
        ancestors.push(ruby.to_symbol(pk))?;
    }
    hash.aset(ruby.to_symbol("ancestor_kinds"), ancestors)?;

    set_optional_str(ruby, &hash, "lang", node.lang.as_deref())?;
    set_optional_str(ruby, &hash, "dest", node.dest.as_deref())?;
    set_optional_str(ruby, &hash, "title", node.title.as_deref())?;
    set_optional_str(ruby, &hash, "id", node.id.as_deref())?;
    set_optional_str(ruby, &hash, "label", node.label.as_deref())?;

    match node.level {
        Some(l) => hash.aset(ruby.to_symbol("level"), l as i64)?,
        None => hash.aset(ruby.to_symbol("level"), ruby.qnil())?,
    }

    match &node.byte_range {
        Some(r) => {
            let ruby_range = ruby.range_new(r.start as i64, r.end as i64, true)?;
            hash.aset(ruby.to_symbol("byte_range"), ruby_range)?;
        }
        None => hash.aset(ruby.to_symbol("byte_range"), ruby.qnil())?,
    }

    let children_arr = ruby.ary_new();
    for child in &node.children {
        let child_hash = node_to_ruby_hash(child, ruby)?;
        children_arr.push(child_hash)?;
    }
    hash.aset(ruby.to_symbol("children"), children_arr)?;

    Ok(hash)
}

fn set_optional_str(ruby: &Ruby, hash: &RHash, key: &str, val: Option<&str>) -> Result<(), Error> {
    match val {
        Some(s) => hash.aset(ruby.to_symbol(key), ruby.str_new(s)),
        None => hash.aset(ruby.to_symbol(key), ruby.qnil()),
    }
}

fn apply_mutations(node: &mut Node, event_obj: Value, _ruby: &Ruby) -> Result<(), Error> {
    node.replacement_html = event_obj.funcall("html", ())?;
    node.replacement_markdown = event_obj.funcall("markdown", ())?;
    node.deleted = event_obj.funcall("deleted?", ())?;

    if matches!(node.kind, "link" | "image") {
        node.new_dest = event_obj.funcall("dest", ())?;
        node.new_title = event_obj.funcall("title", ())?;
    }
    if node.kind == "heading" {
        let level: Option<i64> = event_obj.funcall("level", ())?;
        node.new_level = level.map(|l| l.clamp(1, 6) as u8);
        node.new_id = event_obj.funcall("id", ())?;
    }
    Ok(())
}

pub fn dispatch_handlers(
    nodes: &mut Vec<Node>,
    handlers: &RHash,
    ruby: &Ruby,
) -> Result<(), Error> {
    for node in nodes.iter_mut() {
        dispatch_handlers(&mut node.children, handlers, ruby)?;

        let key = ruby.to_symbol(node.kind);
        let handler_arr: Option<RArray> = handlers.lookup(key)?;
        if let Some(arr) = handler_arr {
            let event_hash = node_to_ruby_hash(node, ruby)?;
            let event_class = get_event_class(ruby)?;
            let event_obj: Value = event_class.funcall("new", (event_hash,))?;

            for handler_val in arr.into_iter() {
                handler_val.funcall::<_, _, Value>("call", (event_obj,))?;
            }

            apply_mutations(node, event_obj, ruby)?;
        }
    }
    Ok(())
}

pub fn native_walk(
    ruby: &Ruby,
    source: String,
    opts_hash: RHash,
    handlers: RHash,
) -> Result<(), Error> {
    let (cm_opts, flags) = build_options(ruby, opts_hash)?;
    let mut ranges = collect_byte_ranges(&source, cm_opts, flags.autolink);
    let parser = Parser::new_ext(&source, cm_opts);
    let events: Vec<Event> = parser.collect();
    let pre = apply_pre_handler_filters(events, &flags);
    let mut tree = build_tree(pre, &mut ranges);
    dispatch_handlers(&mut tree, &handlers, ruby)?;
    Ok(())
}

pub fn native_render_with_handlers(
    ruby: &Ruby,
    source: String,
    opts_hash: RHash,
    handlers: RHash,
) -> Result<String, Error> {
    let (cm_opts, flags) = build_options(ruby, opts_hash)?;
    let mut ranges = collect_byte_ranges(&source, cm_opts, flags.autolink);
    let parser = Parser::new_ext(&source, cm_opts);
    let events: Vec<Event> = parser.collect();
    let pre = apply_pre_handler_filters(events, &flags);
    let mut tree = build_tree(pre, &mut ranges);
    dispatch_handlers(&mut tree, &handlers, ruby)?;
    let owned = tree_to_events(tree, cm_opts, &flags);
    let post = apply_post_handler_filters(owned, &flags);
    let mut buf = String::with_capacity(source.len() * 3 / 2);
    pulldown_cmark::html::push_html(&mut buf, post.into_iter());
    Ok(buf)
}
