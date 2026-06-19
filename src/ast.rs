use alloc::{string::String, vec::Vec};

use crate::span::Span;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NodeMeta {
    pub span: Option<Span>,
}

impl NodeMeta {
    pub const fn new(span: Option<Span>) -> Self {
        Self { span }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Document {
    pub meta: NodeMeta,
    pub children: Vec<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Block {
    Paragraph(Paragraph),
    Heading(Heading),
    ThematicBreak(ThematicBreak),
    BlockQuote(BlockQuote),
    Alert(Alert),
    List(List),
    DescriptionList(DescriptionList),
    CodeBlock(CodeBlock),
    HtmlBlock(HtmlBlock),
    Definition(Definition),
    FootnoteDefinition(FootnoteDefinition),
    Table(Table),
    MathBlock(MathBlock),
    Frontmatter(Frontmatter),
    MdxEsm(MdxEsm),
    MdxExpression(MdxExpression),
    MdxJsx(MdxJsx),
    LeafDirective(LeafDirective),
    ContainerDirective(ContainerDirective),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Paragraph {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Heading {
    pub meta: NodeMeta,
    pub depth: u8,
    pub kind: HeadingKind,
    pub children: Vec<Inline>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeadingKind {
    Atx,
    Setext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThematicBreak {
    pub meta: NodeMeta,
    pub marker: ThematicBreakMarker,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThematicBreakMarker {
    Dash,
    Asterisk,
    Underscore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockQuote {
    pub meta: NodeMeta,
    pub children: Vec<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Alert {
    pub meta: NodeMeta,
    pub kind: AlertKind,
    pub title: Option<String>,
    pub children: Vec<Block>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct List {
    pub meta: NodeMeta,
    pub ordered: bool,
    pub start: Option<u64>,
    pub delimiter: ListDelimiter,
    pub tight: bool,
    pub children: Vec<ListItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListDelimiter {
    Dash,
    Asterisk,
    Plus,
    Period,
    Paren,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListItem {
    pub meta: NodeMeta,
    pub checked: Option<bool>,
    pub children: Vec<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptionList {
    pub meta: NodeMeta,
    pub tight: bool,
    pub children: Vec<DescriptionItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptionItem {
    pub meta: NodeMeta,
    pub term: Vec<Inline>,
    pub details: Vec<DescriptionDetails>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptionDetails {
    pub meta: NodeMeta,
    pub children: Vec<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeBlock {
    pub meta: NodeMeta,
    pub kind: CodeBlockKind,
    pub info: Option<String>,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeBlockKind {
    Fenced { marker: FenceMarker, length: usize },
    Indented,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FenceMarker {
    Backtick,
    Tilde,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlBlock {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Definition {
    pub meta: NodeMeta,
    pub label: String,
    pub identifier: String,
    pub destination: String,
    pub destination_kind: LinkDestinationKind,
    pub title: Option<String>,
    pub title_kind: Option<LinkTitleKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FootnoteDefinition {
    pub meta: NodeMeta,
    pub label: String,
    pub identifier: String,
    pub children: Vec<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Table {
    pub meta: NodeMeta,
    pub alignments: Vec<TableAlignment>,
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableRow {
    pub meta: NodeMeta,
    pub cells: Vec<TableCell>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableCell {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathBlock {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frontmatter {
    pub meta: NodeMeta,
    pub kind: FrontmatterKind,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontmatterKind {
    Yaml,
    Toml,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MdxEsm {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MdxExpression {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MdxJsx {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeafDirective {
    pub meta: NodeMeta,
    pub name: String,
    pub label: Vec<Inline>,
    pub attributes: Vec<DirectiveAttribute>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerDirective {
    pub meta: NodeMeta,
    pub name: String,
    pub label: Vec<Inline>,
    pub attributes: Vec<DirectiveAttribute>,
    pub children: Vec<Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Inline {
    Text(Text),
    Escape(Escape),
    CharacterReference(CharacterReference),
    Emphasis(Emphasis),
    Strong(Strong),
    Underline(Underline),
    Delete(Delete),
    Insert(Insert),
    Mark(Mark),
    Subscript(Subscript),
    Superscript(Superscript),
    Spoiler(Spoiler),
    Shortcode(Shortcode),
    Code(CodeInline),
    Link(Link),
    Image(Image),
    LinkReference(LinkReference),
    ImageReference(ImageReference),
    Autolink(Autolink),
    Html(HtmlInline),
    SoftBreak(SoftBreak),
    LineBreak(LineBreak),
    Math(MathInline),
    FootnoteReference(FootnoteReference),
    InlineFootnote(InlineFootnote),
    WikiLink(WikiLink),
    MdxExpression(MdxExpressionInline),
    MdxJsx(MdxJsxInline),
    TextDirective(TextDirective),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Text {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escape {
    pub meta: NodeMeta,
    pub value: char,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterReference {
    pub meta: NodeMeta,
    pub reference: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Emphasis {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Strong {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Underline {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delete {
    pub meta: NodeMeta,
    pub marker: DeleteMarker,
    pub children: Vec<Inline>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteMarker {
    SingleTilde,
    DoubleTilde,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Insert {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mark {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscript {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Superscript {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Spoiler {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Shortcode {
    pub meta: NodeMeta,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeInline {
    pub meta: NodeMeta,
    pub value: String,
    pub raw: String,
    pub fence_length: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    pub meta: NodeMeta,
    pub destination: String,
    pub destination_kind: LinkDestinationKind,
    pub title: Option<String>,
    pub title_kind: Option<LinkTitleKind>,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Image {
    pub meta: NodeMeta,
    pub destination: String,
    pub destination_kind: LinkDestinationKind,
    pub title: Option<String>,
    pub title_kind: Option<LinkTitleKind>,
    pub alt: Vec<Inline>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkDestinationKind {
    Bare,
    Angle,
    Omitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkTitleKind {
    DoubleQuote,
    SingleQuote,
    Paren,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkReference {
    pub meta: NodeMeta,
    pub identifier: String,
    pub label: String,
    pub kind: ReferenceKind,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageReference {
    pub meta: NodeMeta,
    pub identifier: String,
    pub label: String,
    pub kind: ReferenceKind,
    pub alt: Vec<Inline>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceKind {
    Full,
    Collapsed,
    Shortcut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Autolink {
    pub meta: NodeMeta,
    pub destination: String,
    pub kind: AutolinkKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AutolinkKind {
    /// An angle-bracket autolink `<dest>`. The destination is the raw text
    /// between the brackets; `>` is forbidden in the destination and the
    /// serializer re-emits `<dest>`.
    Angle,
    /// A GFM literal autolink (bare `www.`/`http(s)://`/`mailto:`/`xmpp:` URL
    /// or email). `original` is the raw source text that produced the link
    /// (the visible label); `destination` is the synthesized href (e.g. a
    /// `http://`/`mailto:` prefix may have been prepended). The serializer
    /// re-emits `original`, which re-parses to the same literal.
    GfmLiteral { original: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlInline {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoftBreak {
    pub meta: NodeMeta,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineBreak {
    pub meta: NodeMeta,
    pub kind: LineBreakKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineBreakKind {
    Backslash,
    Spaces,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathInlineKind {
    /// Dollar-fenced inline math (`$…$`, `$$…$$`, …); `dollars` is the fence
    /// length (>=1). Dollar math always renders inline, while a 2-dollar fence
    /// is conventionally treated as display elsewhere in the ecosystem.
    Dollar { dollars: u8 },
    /// Math-code span: `$`…`$`.
    Code,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MathInline {
    pub meta: NodeMeta,
    pub value: String,
    pub kind: MathInlineKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FootnoteReference {
    pub meta: NodeMeta,
    pub label: String,
    pub identifier: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InlineFootnote {
    pub meta: NodeMeta,
    pub children: Vec<Inline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiLink {
    pub meta: NodeMeta,
    pub target: String,
    pub label: String,
    pub label_order: WikiLinkLabelOrder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiLinkLabelOrder {
    AfterPipe,
    BeforePipe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MdxExpressionInline {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MdxJsxInline {
    pub meta: NodeMeta,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextDirective {
    pub meta: NodeMeta,
    pub name: String,
    pub label: Vec<Inline>,
    pub attributes: Vec<DirectiveAttribute>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectiveAttribute {
    pub name: String,
    pub value: Option<String>,
}
