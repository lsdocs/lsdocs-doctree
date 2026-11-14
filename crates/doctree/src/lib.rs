//! Document tree (AST) types: nodes, spans and metadata.
//!
//! A [`Document`] is what a parser produces and what transformers and
//! renderers consume. Every type here crosses process boundaries, so the
//! serialized shape is part of the public API:
//!
//! - Nodes are tagged by a `type` field, e.g. `{"type": "heading", "level": 1,
//!   "children": [...]}`.
//! - Field names are `snake_case`.
//! - Optional fields are omitted when empty, and unknown fields are ignored
//!   when reading, so new fields can be added without breaking older plugins.
//!
//! Formats without a core node kind use [`Node::Custom`].
//!
//! ## The shape every parser produces
//!
//! Parsers for any source format hand transformers the same shape:
//!
//! - **Sections nest.** Content under a title is wrapped in a
//!   [`Node::Section`] whose first child is the title [`Heading`], with
//!   subsections nested inside it. A section's anchor id is
//!   [`Section::id`]; [`Heading::id`] is only for headings outside the
//!   section tree (e.g. inside a block quote). Content before the first
//!   title stays at the top level.
//! - **Ids are explicit or left out.** Parsers set ids the author wrote
//!   (e.g. `{#id}` in Markdown, labels in reStructuredText). The host gives
//!   every other section a unique id derived from its title after parsing.
//! - **Cross-document links carry a [`LinkTarget`].** The host fills it in
//!   for relative links to other documents; parsers set it for references
//!   that are not paths, such as labels.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// An arbitrary structured value, as found in front matter or plugin data.
pub type Value = serde_json::Value;

/// Document-level metadata, such as front matter.
pub type Metadata = BTreeMap<String, Value>;

/// A byte range within the document's source text.
///
/// Offsets count UTF-8 bytes from the start of the source, and `end` is
/// exclusive. The source file is the enclosing [`Document::source`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Span {
    /// Offset of the first byte.
    pub start: u32,
    /// Offset one past the last byte.
    pub end: u32,
}

impl Span {
    /// Creates a span from `start` (inclusive) to `end` (exclusive).
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// A parsed source document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Document {
    /// Path of the source file, relative to the site's source directory,
    /// with `/` separators.
    pub source: String,
    /// Front matter and other document-level data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: Metadata,
    /// Top-level nodes.
    #[serde(default)]
    pub children: Vec<Node>,
}

impl Document {
    /// Creates an empty document for the given source path.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            ..Self::default()
        }
    }

    /// Iterates over every node in the document, depth first, parents before
    /// children.
    pub fn descendants(&self) -> Descendants<'_> {
        Descendants {
            stack: self.children.iter().rev().collect(),
        }
    }

    /// The document's title: its `title` metadata if that is a string, else
    /// the text of its first heading.
    pub fn title(&self) -> Option<String> {
        self.metadata
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                self.descendants().find_map(|n| match n {
                    Node::Heading(h) => Some(plain_text(&h.children)),
                    _ => None,
                })
            })
    }
}

/// The text of `nodes` without markup: text, inline code and image alt
/// text, in order.
pub fn plain_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.value),
            Node::InlineCode(c) => out.push_str(&c.value),
            Node::Image(i) => out.push_str(&i.alt),
            other => out.push_str(&plain_text(other.children())),
        }
    }
    out
}

/// Depth-first iterator returned by [`Document::descendants`] and
/// [`Node::descendants`].
#[derive(Debug, Clone)]
pub struct Descendants<'a> {
    stack: Vec<&'a Node>,
}

impl<'a> Iterator for Descendants<'a> {
    type Item = &'a Node;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack.extend(node.children().iter().rev());
        Some(node)
    }
}

/// A node in the document tree.
///
/// Serialized with a `type` tag holding the [`Node::kind`] name, for core
/// kinds, followed by the variant's fields.
// Deserializing is hand-written (see the `de` module) because serde's
// derived decoder for internally tagged enums buffers every node before
// reading it, which made decoding several times slower than encoding.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    /// A titled section; by convention its first child is a [`Heading`].
    Section(Section),
    /// A heading.
    Heading(Heading),
    /// A paragraph.
    Paragraph(Paragraph),
    /// A bulleted or numbered list whose children are list items.
    List(List),
    /// An item of a [`List`].
    ListItem(ListItem),
    /// A block quote.
    BlockQuote(BlockQuote),
    /// A block of code or preformatted text.
    CodeBlock(CodeBlock),
    /// A table whose children are table rows.
    Table(Table),
    /// A row of a [`Table`] whose children are table cells.
    TableRow(TableRow),
    /// A cell of a [`TableRow`].
    TableCell(TableCell),
    /// A horizontal rule.
    ThematicBreak(ThematicBreak),
    /// Content passed through verbatim to renderers of a matching format,
    /// such as inline HTML in Markdown.
    Raw(Raw),
    /// Plain text.
    Text(Text),
    /// Emphasized (usually italic) text.
    Emphasis(Emphasis),
    /// Strongly emphasized (usually bold) text.
    Strong(Strong),
    /// Inline code.
    InlineCode(InlineCode),
    /// A hyperlink.
    Link(Link),
    /// An image.
    Image(Image),
    /// A hard line break.
    LineBreak(LineBreak),
    /// A callout such as a note or warning.
    Admonition(Admonition),
    /// A list of terms and their descriptions: children alternate between
    /// [`Node::DefinitionTerm`] and one or more
    /// [`Node::DefinitionDescription`]s.
    DefinitionList(DefinitionList),
    /// A term in a [`DefinitionList`].
    DefinitionTerm(DefinitionTerm),
    /// The description of the term before it in a [`DefinitionList`].
    DefinitionDescription(DefinitionDescription),
    /// A reference to a footnote, e.g. `[^1]`.
    FootnoteReference(FootnoteReference),
    /// The text of a footnote.
    FootnoteDefinition(FootnoteDefinition),
    /// A mathematical formula, inline or on its own line.
    Math(Math),
    /// Content (usually an image) with an optional [`Node::Caption`] as its
    /// last child.
    Figure(Figure),
    /// The caption of a [`Figure`].
    Caption(Caption),
    /// A link target with no content of its own, e.g. a reStructuredText
    /// label in front of a paragraph.
    Anchor(Anchor),
    /// A node kind defined by a plugin rather than by lsdocs.
    Custom(Custom),
}

impl Node {
    /// The name plugins use to select this node, e.g. in transformer
    /// capabilities: the `type` tag for core kinds (`"heading"`), or
    /// `"custom:<kind>"` for custom nodes.
    pub fn kind(&self) -> std::borrow::Cow<'static, str> {
        use std::borrow::Cow::{Borrowed, Owned};
        Borrowed(match self {
            Node::Section(_) => "section",
            Node::Heading(_) => "heading",
            Node::Paragraph(_) => "paragraph",
            Node::List(_) => "list",
            Node::ListItem(_) => "list_item",
            Node::BlockQuote(_) => "block_quote",
            Node::CodeBlock(_) => "code_block",
            Node::Table(_) => "table",
            Node::TableRow(_) => "table_row",
            Node::TableCell(_) => "table_cell",
            Node::ThematicBreak(_) => "thematic_break",
            Node::Raw(_) => "raw",
            Node::Text(_) => "text",
            Node::Emphasis(_) => "emphasis",
            Node::Strong(_) => "strong",
            Node::InlineCode(_) => "inline_code",
            Node::Link(_) => "link",
            Node::Image(_) => "image",
            Node::LineBreak(_) => "line_break",
            Node::Admonition(_) => "admonition",
            Node::DefinitionList(_) => "definition_list",
            Node::DefinitionTerm(_) => "definition_term",
            Node::DefinitionDescription(_) => "definition_description",
            Node::FootnoteReference(_) => "footnote_reference",
            Node::FootnoteDefinition(_) => "footnote_definition",
            Node::Math(_) => "math",
            Node::Figure(_) => "figure",
            Node::Caption(_) => "caption",
            Node::Anchor(_) => "anchor",
            Node::Custom(c) => return Owned(format!("custom:{}", c.kind)),
        })
    }

    /// Where this node came from in the source, if known.
    pub fn span(&self) -> Option<Span> {
        match self {
            Node::Section(n) => n.span,
            Node::Heading(n) => n.span,
            Node::Paragraph(n) => n.span,
            Node::List(n) => n.span,
            Node::ListItem(n) => n.span,
            Node::BlockQuote(n) => n.span,
            Node::CodeBlock(n) => n.span,
            Node::Table(n) => n.span,
            Node::TableRow(n) => n.span,
            Node::TableCell(n) => n.span,
            Node::ThematicBreak(n) => n.span,
            Node::Raw(n) => n.span,
            Node::Text(n) => n.span,
            Node::Emphasis(n) => n.span,
            Node::Strong(n) => n.span,
            Node::InlineCode(n) => n.span,
            Node::Link(n) => n.span,
            Node::Image(n) => n.span,
            Node::LineBreak(n) => n.span,
            Node::Admonition(n) => n.span,
            Node::DefinitionList(n) => n.span,
            Node::DefinitionTerm(n) => n.span,
            Node::DefinitionDescription(n) => n.span,
            Node::FootnoteReference(n) => n.span,
            Node::FootnoteDefinition(n) => n.span,
            Node::Math(n) => n.span,
            Node::Figure(n) => n.span,
            Node::Caption(n) => n.span,
            Node::Anchor(n) => n.span,
            Node::Custom(n) => n.span,
        }
    }

    /// This node's children; empty for leaf kinds.
    pub fn children(&self) -> &[Node] {
        match self {
            Node::Section(n) => &n.children,
            Node::Heading(n) => &n.children,
            Node::Paragraph(n) => &n.children,
            Node::List(n) => &n.children,
            Node::ListItem(n) => &n.children,
            Node::BlockQuote(n) => &n.children,
            Node::Table(n) => &n.children,
            Node::TableRow(n) => &n.children,
            Node::TableCell(n) => &n.children,
            Node::Emphasis(n) => &n.children,
            Node::Strong(n) => &n.children,
            Node::Link(n) => &n.children,
            Node::Admonition(n) => &n.children,
            Node::DefinitionList(n) => &n.children,
            Node::DefinitionTerm(n) => &n.children,
            Node::DefinitionDescription(n) => &n.children,
            Node::FootnoteDefinition(n) => &n.children,
            Node::Figure(n) => &n.children,
            Node::Caption(n) => &n.children,
            Node::Custom(n) => &n.children,
            Node::CodeBlock(_)
            | Node::ThematicBreak(_)
            | Node::Raw(_)
            | Node::Text(_)
            | Node::InlineCode(_)
            | Node::Image(_)
            | Node::LineBreak(_)
            | Node::FootnoteReference(_)
            | Node::Math(_)
            | Node::Anchor(_) => &[],
        }
    }

    /// Mutable access to this node's children, or `None` for leaf kinds.
    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::Section(n) => Some(&mut n.children),
            Node::Heading(n) => Some(&mut n.children),
            Node::Paragraph(n) => Some(&mut n.children),
            Node::List(n) => Some(&mut n.children),
            Node::ListItem(n) => Some(&mut n.children),
            Node::BlockQuote(n) => Some(&mut n.children),
            Node::Table(n) => Some(&mut n.children),
            Node::TableRow(n) => Some(&mut n.children),
            Node::TableCell(n) => Some(&mut n.children),
            Node::Emphasis(n) => Some(&mut n.children),
            Node::Strong(n) => Some(&mut n.children),
            Node::Link(n) => Some(&mut n.children),
            Node::Admonition(n) => Some(&mut n.children),
            Node::DefinitionList(n) => Some(&mut n.children),
            Node::DefinitionTerm(n) => Some(&mut n.children),
            Node::DefinitionDescription(n) => Some(&mut n.children),
            Node::FootnoteDefinition(n) => Some(&mut n.children),
            Node::Figure(n) => Some(&mut n.children),
            Node::Caption(n) => Some(&mut n.children),
            Node::Custom(n) => Some(&mut n.children),
            Node::CodeBlock(_)
            | Node::ThematicBreak(_)
            | Node::Raw(_)
            | Node::Text(_)
            | Node::InlineCode(_)
            | Node::Image(_)
            | Node::LineBreak(_)
            | Node::FootnoteReference(_)
            | Node::Math(_)
            | Node::Anchor(_) => None,
        }
    }

    /// Iterates over this node and everything below it, depth first.
    pub fn descendants(&self) -> Descendants<'_> {
        Descendants { stack: vec![self] }
    }
}

mod de {
    //! A [`Deserialize`] for [`Node`] that reads the `type` tag first, then
    //! decodes the rest of the map straight into the variant's struct.
    //!
    //! Our encoder always writes `type` first, so this is the usual path. A
    //! map whose first key is something else (e.g. a node built in Python or
    //! JavaScript with its keys in another order) is buffered as JSON and
    //! decoded from that.

    use std::fmt;

    use serde::de::value::MapAccessDeserializer;
    use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};

    use super::*;

    /// Decodes the variant named `tag` from `d`, a map without the tag.
    macro_rules! variants {
        ($tag:expr, $d:expr, $( $name:literal => $variant:ident ),* $(,)?) => {
            match $tag {
                $( $name => $variant::deserialize($d).map(Node::$variant), )*
                other => Err(de::Error::unknown_variant(other, &[$($name),*])),
            }
        };
    }

    fn variant<'de, D: Deserializer<'de>>(tag: &str, d: D) -> Result<Node, D::Error> {
        variants!(tag, d,
            "section" => Section,
            "heading" => Heading,
            "paragraph" => Paragraph,
            "list" => List,
            "list_item" => ListItem,
            "block_quote" => BlockQuote,
            "code_block" => CodeBlock,
            "table" => Table,
            "table_row" => TableRow,
            "table_cell" => TableCell,
            "thematic_break" => ThematicBreak,
            "raw" => Raw,
            "text" => Text,
            "emphasis" => Emphasis,
            "strong" => Strong,
            "inline_code" => InlineCode,
            "link" => Link,
            "image" => Image,
            "line_break" => LineBreak,
            "admonition" => Admonition,
            "definition_list" => DefinitionList,
            "definition_term" => DefinitionTerm,
            "definition_description" => DefinitionDescription,
            "footnote_reference" => FootnoteReference,
            "footnote_definition" => FootnoteDefinition,
            "math" => Math,
            "figure" => Figure,
            "caption" => Caption,
            "anchor" => Anchor,
            "custom" => Custom,
        )
    }

    /// A map key, without allocating when it is `type`.
    enum Key {
        Type,
        Other(String),
    }

    impl<'de> Deserialize<'de> for Key {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct KeyVisitor;
            impl Visitor<'_> for KeyVisitor {
                type Value = Key;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a field name")
                }
                fn visit_str<E: de::Error>(self, v: &str) -> Result<Key, E> {
                    Ok(if v == "type" {
                        Key::Type
                    } else {
                        Key::Other(v.to_owned())
                    })
                }
            }
            d.deserialize_str(KeyVisitor)
        }
    }

    /// A node's `type`, borrowed from the input when possible.
    struct Tag<'de>(std::borrow::Cow<'de, str>);

    impl<'de> Deserialize<'de> for Tag<'de> {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct TagVisitor;
            impl<'de> Visitor<'de> for TagVisitor {
                type Value = Tag<'de>;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a node type")
                }
                fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Tag<'de>, E> {
                    Ok(Tag(v.into()))
                }
                fn visit_str<E: de::Error>(self, v: &str) -> Result<Tag<'de>, E> {
                    Ok(Tag(v.to_owned().into()))
                }
            }
            d.deserialize_str(TagVisitor)
        }
    }

    struct NodeVisitor;

    impl<'de> Visitor<'de> for NodeVisitor {
        type Value = Node;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a document node: a map with a `type` field")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
            let first = match map.next_key::<Key>()? {
                None => return Err(de::Error::missing_field("type")),
                Some(Key::Type) => {
                    let Tag(tag) = map.next_value()?;
                    return variant(&tag, MapAccessDeserializer::new(map));
                }
                Some(Key::Other(key)) => key,
            };
            // Slow path: buffer the whole map, then find the tag.
            let mut fields = serde_json::Map::new();
            fields.insert(first, map.next_value()?);
            while let Some((key, value)) = map.next_entry::<String, Value>()? {
                fields.insert(key, value);
            }
            let tag = match fields.remove("type") {
                Some(Value::String(tag)) => tag,
                Some(_) => return Err(de::Error::custom("`type` must be a string")),
                None => return Err(de::Error::missing_field("type")),
            };
            variant(&tag, Value::Object(fields)).map_err(de::Error::custom)
        }
    }

    impl<'de> Deserialize<'de> for Node {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            d.deserialize_map(NodeVisitor)
        }
    }
}

/// Defines a node struct with an optional `span` plus the given fields, and
/// a `From` conversion into [`Node`].
macro_rules! node_struct {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$fmeta:meta])* $field:ident : $ty:ty ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        pub struct $name {
            /// Where this node came from in the source, if known.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub span: Option<Span>,
            $( $(#[$fmeta])* pub $field: $ty, )*
        }

        impl From<$name> for Node {
            fn from(n: $name) -> Self {
                Node::$name(n)
            }
        }
    };
}

node_struct! {
    /// See [`Node::Section`].
    Section {
        /// Anchor identifier for linking to the section. Set by the parser
        /// if the author gave one; otherwise the host derives one from the
        /// title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Content of the section.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::Heading`].
    Heading {
        /// Heading level, from 1 (top) to 6. For a section title this is
        /// the level as written, which may skip levels; the nesting of
        /// sections is the structure.
        level: u8,
        /// Anchor identifier, only for headings that are not a section's
        /// title (a section's anchor is [`Section::id`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Inline content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::Paragraph`].
    Paragraph {
        /// Inline content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::List`].
    List {
        /// Whether items are numbered.
        #[serde(default)]
        ordered: bool,
        /// Number of the first item of an ordered list, if not 1.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<u64>,
        /// The list's items.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::ListItem`].
    ListItem {
        /// For task list items, whether the box is checked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
        /// Content of the item.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::BlockQuote`].
    BlockQuote {
        /// Quoted content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::CodeBlock`].
    CodeBlock {
        /// Language name, e.g. `"rust"`, if given.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        /// Anything after the language on a Markdown fence's info line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        info: Option<String>,
        /// The code itself.
        value: String,
    }
}

node_struct! {
    /// See [`Node::Table`].
    Table {
        /// Alignment of each column.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        align: Vec<Align>,
        /// The table's rows.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::TableRow`].
    TableRow {
        /// Whether this is a header row.
        #[serde(default)]
        header: bool,
        /// The row's cells.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::TableCell`].
    TableCell {
        /// Content of the cell.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::ThematicBreak`].
    ThematicBreak {}
}

node_struct! {
    /// See [`Node::Raw`].
    Raw {
        /// Output format the content is meant for, e.g. `"html"`.
        format: String,
        /// The verbatim content.
        value: String,
    }
}

node_struct! {
    /// See [`Node::Text`].
    Text {
        /// The text.
        value: String,
    }
}

node_struct! {
    /// See [`Node::Emphasis`].
    Emphasis {
        /// Emphasized content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::Strong`].
    Strong {
        /// Strongly emphasized content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::InlineCode`].
    InlineCode {
        /// The code.
        value: String,
    }
}

node_struct! {
    /// See [`Node::Link`].
    Link {
        /// Link target.
        url: String,
        /// Link title (often shown as a tooltip).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Where the link points inside the site, if it is a
        /// cross-reference. Renderers prefer it over `url`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<LinkTarget>,
        /// Link text.
        #[serde(default)]
        children: Vec<Node>,
    }
}

/// The site-internal destination of a [`Link`].
///
/// A resolved target names a document, an anchor, or both (an anchor alone
/// means the linking document). An unresolved one has only a `label`, to be
/// looked up among the anchors of every document, e.g. by a transformer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LinkTarget {
    /// Source path of the target document, relative to the source
    /// directory with `/` separators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    /// Anchor id within the target document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// A label naming the target, not yet resolved to a document and
    /// anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

node_struct! {
    /// See [`Node::Image`].
    Image {
        /// Image location.
        url: String,
        /// Alternative text.
        #[serde(default)]
        alt: String,
        /// Image title (often shown as a tooltip).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    }
}

node_struct! {
    /// See [`Node::LineBreak`].
    LineBreak {}
}

node_struct! {
    /// See [`Node::Admonition`].
    Admonition {
        /// What kind of callout, e.g. `"note"`, `"tip"`, `"warning"`.
        kind: String,
        /// A title replacing the kind's default one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::DefinitionList`].
    DefinitionList {
        /// Terms and descriptions, in order.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::DefinitionTerm`].
    DefinitionTerm {
        /// Inline content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::DefinitionDescription`].
    DefinitionDescription {
        /// Content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::FootnoteReference`].
    FootnoteReference {
        /// The footnote's label, e.g. `"1"` or `"note"`.
        label: String,
    }
}

node_struct! {
    /// See [`Node::FootnoteDefinition`].
    FootnoteDefinition {
        /// The label references use.
        label: String,
        /// Content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::Math`].
    Math {
        /// The formula, e.g. in TeX notation.
        value: String,
        /// Whether it is set on its own line rather than inline.
        #[serde(default)]
        display: bool,
    }
}

node_struct! {
    /// See [`Node::Figure`].
    Figure {
        /// Anchor identifier, e.g. for linking to the figure.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Content, then an optional caption.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::Caption`].
    Caption {
        /// Content.
        #[serde(default)]
        children: Vec<Node>,
    }
}

node_struct! {
    /// See [`Node::Anchor`].
    Anchor {
        /// The anchor identifier.
        id: String,
    }
}

node_struct! {
    /// See [`Node::Custom`].
    Custom {
        /// The plugin-defined kind, e.g. `"mermaid"`. Selected as
        /// `"custom:<kind>"`.
        kind: String,
        /// Arbitrary attributes.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        attrs: BTreeMap<String, Value>,
        /// Child nodes, if any.
        #[serde(default)]
        children: Vec<Node>,
    }
}

/// Horizontal alignment of a table column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Align {
    /// No alignment specified.
    #[default]
    None,
    /// Left-aligned.
    Left,
    /// Centered.
    Center,
    /// Right-aligned.
    Right,
}

/// Shorthand for a [`Text`] node.
pub fn text(value: impl Into<String>) -> Node {
    Text {
        span: None,
        value: value.into(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Document {
        let mut doc = Document::new("guide/intro.md");
        doc.metadata.insert("title".into(), json!("Intro"));
        doc.children = vec![
            Heading {
                span: Some(Span::new(0, 8)),
                level: 1,
                id: Some("intro".into()),
                children: vec![text("Intro")],
            }
            .into(),
            Paragraph {
                span: None,
                children: vec![
                    text("See "),
                    Link {
                        url: "https://example.com".into(),
                        children: vec![text("here")],
                        ..Link::default()
                    }
                    .into(),
                ],
            }
            .into(),
            Custom {
                kind: "mermaid".into(),
                attrs: BTreeMap::from([("source".into(), json!("graph TD; A-->B"))]),
                ..Custom::default()
            }
            .into(),
        ];
        doc
    }

    #[test]
    fn nodes_are_tagged_by_type_and_omit_empty_fields() {
        let heading = &sample().children[0];
        assert_eq!(
            serde_json::to_value(heading).unwrap(),
            json!({
                "type": "heading",
                "span": {"start": 0, "end": 8},
                "level": 1,
                "id": "intro",
                "children": [{"type": "text", "value": "Intro"}],
            })
        );
    }

    #[test]
    fn json_round_trip() {
        let doc = sample();
        let json = serde_json::to_string(&doc).unwrap();
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
    }

    #[test]
    fn missing_optional_fields_and_unknown_fields_are_accepted() {
        let node: Node = serde_json::from_value(json!({
            "type": "paragraph",
            "added_in_a_later_version": true,
        }))
        .unwrap();
        assert_eq!(node, Paragraph::default().into());
    }

    #[test]
    fn kind_matches_the_type_tag() {
        for node in sample().descendants() {
            let kind = node.kind();
            match node {
                Node::Custom(c) => assert_eq!(kind, format!("custom:{}", c.kind)),
                _ => {
                    let tag = serde_json::to_value(node).unwrap()["type"].clone();
                    assert_eq!(json!(kind), tag);
                }
            }
        }
    }

    #[test]
    fn titles_and_plain_text() {
        let mut doc = sample();
        assert_eq!(doc.title().as_deref(), Some("Intro"));
        doc.metadata.clear();
        doc.children[0] = Heading {
            level: 1,
            children: vec![
                text("A "),
                InlineCode {
                    value: "b".into(),
                    ..InlineCode::default()
                }
                .into(),
                Image {
                    alt: " c".into(),
                    ..Image::default()
                }
                .into(),
            ],
            ..Heading::default()
        }
        .into();
        assert_eq!(doc.title().as_deref(), Some("A b c"));
        assert_eq!(Document::new("x.md").title(), None);
    }

    #[test]
    fn nodes_decode_whatever_the_key_order() {
        let tag_first =
            json!({"type": "heading", "level": 2, "children": [{"type": "text", "value": "A"}]});
        let tag_last =
            json!({"level": 2, "children": [{"value": "A", "type": "text"}], "type": "heading"});
        let a: Node = serde_json::from_value(tag_first).unwrap();
        let b: Node = serde_json::from_value(tag_last).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.kind(), "heading");
        let bad = serde_json::from_value::<Node>(json!({"type": "nope"})).unwrap_err();
        assert!(bad.to_string().contains("unknown variant `nope`"), "{bad}");
        let missing = serde_json::from_value::<Node>(json!({"level": 1})).unwrap_err();
        assert!(missing.to_string().contains("type"), "{missing}");
    }

    #[test]
    fn descendants_are_depth_first() {
        let kinds: Vec<_> = sample().descendants().map(|n| n.kind()).collect();
        assert_eq!(
            kinds,
            [
                "heading",
                "text",
                "paragraph",
                "text",
                "link",
                "text",
                "custom:mermaid"
            ]
        );
    }
}
