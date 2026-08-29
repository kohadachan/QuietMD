use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleSpan {
    pub start: u32,
    pub length: u32,
    pub style: TextStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Quote,
    ListItem { depth: usize },
    Code,
    Rule,
    TableRow { header: bool },
    Image { source: String, alt: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    pub spans: Vec<StyleSpan>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy)]
struct ListState {
    ordered: bool,
    next: u64,
}

struct Draft {
    kind: BlockKind,
    text: String,
    spans: Vec<StyleSpan>,
}

impl Draft {
    fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            text: String::new(),
            spans: Vec::new(),
        }
    }

    fn append(&mut self, value: &str, style: TextStyle) {
        if value.is_empty() {
            return;
        }

        let start = utf16_len(&self.text);
        self.text.push_str(value);
        let length = utf16_len(value);
        if length == 0 || style == TextStyle::default() {
            return;
        }

        if let Some(last) = self.spans.last_mut()
            && last.style == style
            && last.start + last.length == start
        {
            last.length += length;
            return;
        }

        self.spans.push(StyleSpan {
            start,
            length,
            style,
        });
    }
}

#[derive(Default)]
struct TableState {
    header: bool,
    row: Vec<String>,
    cell: Option<String>,
}

struct ImageState {
    source: String,
    alt: String,
}

pub fn parse(markdown: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_GFM);

    let mut blocks = Vec::new();
    let mut draft: Option<Draft> = None;
    let mut style = TextStyle::default();
    let mut lists: Vec<ListState> = Vec::new();
    let mut pending_item_prefix: Option<String> = None;
    let mut quote_depth = 0usize;
    let mut table: Option<TableState> = None;
    let mut image: Option<ImageState> = None;

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if table.is_none() && image.is_none() {
                        finish(&mut draft, &mut blocks);
                        let kind = if !lists.is_empty() {
                            BlockKind::ListItem { depth: lists.len() }
                        } else if quote_depth > 0 {
                            BlockKind::Quote
                        } else {
                            BlockKind::Paragraph
                        };
                        let mut next = Draft::new(kind);
                        if let Some(prefix) = pending_item_prefix.take() {
                            next.append(&prefix, TextStyle::default());
                        }
                        draft = Some(next);
                    }
                }
                Tag::Heading { level, .. } => {
                    finish(&mut draft, &mut blocks);
                    draft = Some(Draft::new(BlockKind::Heading(heading_number(level))));
                }
                Tag::BlockQuote(_) => quote_depth += 1,
                Tag::CodeBlock(kind) => {
                    finish(&mut draft, &mut blocks);
                    let mut next = Draft::new(BlockKind::Code);
                    if let CodeBlockKind::Fenced(language) = kind
                        && !language.is_empty()
                    {
                        let label = format!("{}\n\n", language.trim());
                        next.append(
                            &label,
                            TextStyle {
                                italic: true,
                                ..TextStyle::default()
                            },
                        );
                    }
                    draft = Some(next);
                }
                Tag::List(start) => lists.push(ListState {
                    ordered: start.is_some(),
                    next: start.unwrap_or(1),
                }),
                Tag::Item => {
                    finish(&mut draft, &mut blocks);
                    if let Some(list) = lists.last_mut() {
                        pending_item_prefix = Some(if list.ordered {
                            let prefix = format!("{}.  ", list.next);
                            list.next += 1;
                            prefix
                        } else {
                            "•  ".to_string()
                        });
                    }
                }
                Tag::Table(_) => table = Some(TableState::default()),
                Tag::TableHead => {
                    if let Some(state) = table.as_mut() {
                        state.header = true;
                        state.row.clear();
                    }
                }
                Tag::TableRow => {
                    if let Some(state) = table.as_mut() {
                        state.row.clear();
                    }
                }
                Tag::TableCell => {
                    if let Some(state) = table.as_mut() {
                        state.cell = Some(String::new());
                    }
                }
                Tag::Emphasis => style.italic = true,
                Tag::Strong => style.bold = true,
                Tag::Strikethrough => style.strike = true,
                Tag::Link { .. } => style.link = true,
                Tag::Image { dest_url, .. } => {
                    finish(&mut draft, &mut blocks);
                    image = Some(ImageState {
                        source: dest_url.into_string(),
                        alt: String::new(),
                    });
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock => {
                    finish(&mut draft, &mut blocks)
                }
                TagEnd::BlockQuote(_) => quote_depth = quote_depth.saturating_sub(1),
                TagEnd::List(_) => {
                    finish(&mut draft, &mut blocks);
                    lists.pop();
                }
                TagEnd::Item => finish(&mut draft, &mut blocks),
                TagEnd::TableHead => {
                    if let Some(state) = table.as_mut() {
                        let text = format!("│ {} │", state.row.join(" │ "));
                        blocks.push(Block {
                            kind: BlockKind::TableRow { header: true },
                            text,
                            spans: Vec::new(),
                        });
                        state.header = false;
                    }
                }
                TagEnd::TableCell => {
                    if let Some(state) = table.as_mut()
                        && let Some(cell) = state.cell.take()
                    {
                        state.row.push(cell.trim().to_string());
                    }
                }
                TagEnd::TableRow => {
                    if let Some(state) = table.as_ref() {
                        let text = format!("│ {} │", state.row.join(" │ "));
                        blocks.push(Block {
                            kind: BlockKind::TableRow {
                                header: state.header,
                            },
                            text,
                            spans: Vec::new(),
                        });
                    }
                }
                TagEnd::Table => table = None,
                TagEnd::Emphasis => style.italic = false,
                TagEnd::Strong => style.bold = false,
                TagEnd::Strikethrough => style.strike = false,
                TagEnd::Link => style.link = false,
                TagEnd::Image => {
                    if let Some(value) = image.take() {
                        blocks.push(Block {
                            kind: BlockKind::Image {
                                source: value.source,
                                alt: value.alt,
                            },
                            text: String::new(),
                            spans: Vec::new(),
                        });
                    }
                }
                _ => {}
            },
            Event::Text(value) => {
                if let Some(value_image) = image.as_mut() {
                    value_image.alt.push_str(&value);
                } else if let Some(state) = table.as_mut()
                    && let Some(cell) = state.cell.as_mut()
                {
                    cell.push_str(&value);
                } else {
                    ensure_draft(&mut draft, &lists, quote_depth, &mut pending_item_prefix);
                    if let Some(current) = draft.as_mut() {
                        current.append(&value, style);
                    }
                }
            }
            Event::Code(value) => {
                if let Some(state) = table.as_mut()
                    && let Some(cell) = state.cell.as_mut()
                {
                    cell.push_str(&value);
                } else {
                    let previous = style.code;
                    style.code = true;
                    ensure_draft(&mut draft, &lists, quote_depth, &mut pending_item_prefix);
                    if let Some(current) = draft.as_mut() {
                        current.append(&value, style);
                    }
                    style.code = previous;
                }
            }
            Event::SoftBreak => append_break(&mut draft, " "),
            Event::HardBreak => append_break(&mut draft, "\n"),
            Event::Rule => {
                finish(&mut draft, &mut blocks);
                blocks.push(Block {
                    kind: BlockKind::Rule,
                    text: String::new(),
                    spans: Vec::new(),
                });
            }
            Event::TaskListMarker(checked) => {
                ensure_draft(&mut draft, &lists, quote_depth, &mut pending_item_prefix);
                if let Some(current) = draft.as_mut() {
                    current.append(if checked { "☑ " } else { "☐ " }, style);
                }
            }
            Event::Html(value) | Event::InlineHtml(value) => {
                ensure_draft(&mut draft, &lists, quote_depth, &mut pending_item_prefix);
                let literal = TextStyle {
                    code: true,
                    ..style
                };
                if let Some(current) = draft.as_mut() {
                    current.append(&value, literal);
                }
            }
            Event::InlineMath(value) | Event::DisplayMath(value) => {
                ensure_draft(&mut draft, &lists, quote_depth, &mut pending_item_prefix);
                if let Some(current) = draft.as_mut() {
                    current.append(
                        &value,
                        TextStyle {
                            code: true,
                            ..style
                        },
                    );
                }
            }
            Event::FootnoteReference(value) => {
                ensure_draft(&mut draft, &lists, quote_depth, &mut pending_item_prefix);
                if let Some(current) = draft.as_mut() {
                    current.append(&format!("[{}]", value), style);
                }
            }
        }
    }

    finish(&mut draft, &mut blocks);
    Document { blocks }
}

fn ensure_draft(
    draft: &mut Option<Draft>,
    lists: &[ListState],
    quote_depth: usize,
    pending_item_prefix: &mut Option<String>,
) {
    if draft.is_some() {
        return;
    }

    let kind = if !lists.is_empty() {
        BlockKind::ListItem { depth: lists.len() }
    } else if quote_depth > 0 {
        BlockKind::Quote
    } else {
        BlockKind::Paragraph
    };
    let mut next = Draft::new(kind);
    if let Some(prefix) = pending_item_prefix.take() {
        next.append(&prefix, TextStyle::default());
    }
    *draft = Some(next);
}

fn append_break(draft: &mut Option<Draft>, value: &str) {
    if let Some(current) = draft.as_mut() {
        current.append(value, TextStyle::default());
    }
}

fn finish(draft: &mut Option<Draft>, blocks: &mut Vec<Block>) {
    let Some(mut current) = draft.take() else {
        return;
    };

    while current.text.ends_with(['\r', '\n']) {
        current.text.pop();
    }

    let text_len = utf16_len(&current.text);
    for span in &mut current.spans {
        span.length = span.length.min(text_len.saturating_sub(span.start));
    }
    current.spans.retain(|span| span.length > 0);

    if !current.text.is_empty() || matches!(current.kind, BlockKind::Code) {
        blocks.push(Block {
            kind: current.kind,
            text: current.text,
            spans: current.spans,
        });
    }
}

fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn utf16_len(value: &str) -> u32 {
    value.encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heading_and_inline_styles() {
        let doc = parse("# 見出し\n\n**太字** と `code`");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(doc.blocks[0].text, "見出し");
        assert_eq!(doc.blocks[1].text, "太字 と code");
        assert!(doc.blocks[1].spans.iter().any(|span| span.style.bold));
        assert!(doc.blocks[1].spans.iter().any(|span| span.style.code));
    }

    #[test]
    fn parses_lists_and_task_markers() {
        let doc = parse("- first\n- [x] done\n1. one\n2. two");
        assert_eq!(doc.blocks.len(), 4);
        assert_eq!(doc.blocks[0].text, "•  first");
        assert_eq!(doc.blocks[1].text, "•  ☑ done");
        assert_eq!(doc.blocks[2].text, "1.  one");
        assert_eq!(doc.blocks[3].text, "2.  two");
    }

    #[test]
    fn parses_table_rows_and_images() {
        let doc = parse("| A | B |\n|---|---|\n| 1 | 2 |\n\n![alt](image.png)");
        assert!(matches!(
            doc.blocks[0].kind,
            BlockKind::TableRow { header: true }
        ));
        assert!(matches!(
            doc.blocks[2].kind,
            BlockKind::Image { ref source, ref alt }
                if source == "image.png" && alt == "alt"
        ));
    }

    #[test]
    fn keeps_inline_code_inside_table_cells() {
        let doc = parse(
            "| GameObject | Timeline |\n|---|---|\n| `Rocket` | RocketTail.playable |\n| `SW_RocketTail` | RocketTail |\n\nAfter",
        );
        assert_eq!(doc.blocks.len(), 4);
        assert_eq!(doc.blocks[1].text, "│ Rocket │ RocketTail.playable │");
        assert_eq!(doc.blocks[2].text, "│ SW_RocketTail │ RocketTail │");
        assert_eq!(doc.blocks[3].text, "After");
    }

    #[test]
    fn keeps_raw_html_as_literal_text() {
        let doc = parse("before <script>alert(1)</script> after");
        assert_eq!(doc.blocks[0].text, "before <script>alert(1)</script> after");
    }

    #[test]
    fn separates_fenced_code_language_and_removes_the_trailing_blank_line() {
        let doc = parse("```text\nfirst\nsecond\n```\n");
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].kind, BlockKind::Code);
        assert_eq!(doc.blocks[0].text, "text\n\nfirst\nsecond");
        assert_eq!(doc.blocks[0].spans.len(), 1);
        assert!(doc.blocks[0].spans[0].style.italic);
        assert_eq!(doc.blocks[0].spans[0].length, 6);
    }
}
