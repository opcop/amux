//! Built-in file preview — renders Markdown, code, and plain text
//! using GPUI native elements (no WebView).

#[cfg(feature = "gpui")]
use gpui::{
    rgb, px, div, prelude::*, AnyElement, FontWeight, IntoElement,
    ParentElement, Styled,
};

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;

/// State for the preview panel
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct PreviewState {
    /// File path being previewed
    pub file_path: String,
    /// File name (for display)
    pub file_name: String,
    /// Parsed content elements
    pub elements: Vec<PreviewElement>,
    /// Scroll offset in pixels
    pub scroll_offset: f32,
    /// Panel width in pixels (user-resizable)
    pub width: f32,
}

/// A renderable element in the preview
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub enum PreviewElement {
    Heading { level: u8, text: String },
    Paragraph { spans: Vec<TextSpan> },
    CodeBlock {
        language: String,
        /// Pre-formatted lines: (display_text, dominant_color). Computed once at load time.
        formatted_lines: Vec<(String, u32)>,
        total_lines: usize,
    },
    ListItem { depth: u8, ordered: bool, index: usize, spans: Vec<TextSpan> },
    HorizontalRule,
    Blockquote { spans: Vec<TextSpan> },
    Table { headers: Vec<Vec<TextSpan>>, rows: Vec<Vec<Vec<TextSpan>>> },
    BlankLine,
}

/// Inline text with styling
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct TextSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link_url: Option<String>,
}

/// State for the file picker (Ctrl+P)
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct FilePickerState {
    pub query: String,
    /// Cached full file list (scanned once on open)
    all_files: Vec<String>,
    /// Filtered matches for current query
    pub matches: Vec<String>,
    pub selected_index: usize,
}

// ─── Markdown Parsing ──────────────────────────────────────────

#[cfg(feature = "gpui")]
impl PreviewState {
    /// Load and parse a file for preview
    pub fn load(file_path: &str) -> Option<Self> {
        let content = std::fs::read_to_string(file_path).ok()?;
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());

        let elements = if file_path.ends_with(".md") || file_path.ends_with(".markdown") {
            parse_markdown(&content)
        } else {
            let lang = detect_language(file_path);
            vec![format_code_block(&lang, &content)]
        };

        Some(Self {
            file_path: file_path.to_string(),
            file_name,
            elements,
            scroll_offset: 0.0,
            width: 680.0,
        })
    }
}

/// Pre-format a code block: highlight + format into (display_text, color) pairs.
/// Done once at load time so render is zero-cost.
#[cfg(feature = "gpui")]
fn format_code_block(language: &str, code: &str) -> PreviewElement {
    let max_lines = 300;
    let all_lines: Vec<&str> = code.lines().collect();
    let total = all_lines.len();
    let render_count = total.min(max_lines);
    let gutter_w = if total >= 1000 { 5 } else if total >= 100 { 4 } else { 3 };
    let formatted: Vec<(String, u32)> = all_lines[..render_count].iter().enumerate().map(|(i, line)| {
        let tokens = highlight_line(line, language);
        let line_num = format!("{:>width$} ", i + 1, width = gutter_w);
        let code_text: String = tokens.iter().map(|t| t.text.as_str()).collect();
        let color = tokens.iter()
            .find(|t| t.color != 0xc5c8c6 && t.color != 0x969896)
            .map(|t| t.color)
            .unwrap_or(0xc5c8c6);
        (format!("{}{}", line_num, code_text), color)
    }).collect();
    PreviewElement::CodeBlock {
        language: language.to_string(),
        formatted_lines: formatted,
        total_lines: total,
    }
}

/// Parse Markdown content into PreviewElements using pulldown-cmark
#[cfg(feature = "gpui")]
fn parse_markdown(content: &str) -> Vec<PreviewElement> {
    use pulldown_cmark::{Parser, Options, Event, Tag, TagEnd, HeadingLevel, CodeBlockKind};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(content, options);
    let mut elements = Vec::new();

    // State tracking
    let mut current_spans: Vec<TextSpan> = Vec::new();
    let mut bold = false;
    let mut italic = false;
    let mut in_code_span = false;
    let mut link_url: Option<String> = None;
    let mut in_heading: Option<u8> = None;
    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut code_block_content = String::new();
    let mut in_blockquote = false;
    let mut list_stack: Vec<(bool, usize)> = Vec::new(); // (ordered, next_index)

    // Table state
    let mut in_table = false;
    let mut in_table_head = false;
    let mut table_headers: Vec<Vec<TextSpan>> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<TextSpan>>> = Vec::new();
    let mut current_row: Vec<Vec<TextSpan>> = Vec::new();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    in_heading = Some(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    });
                    current_spans.clear();
                }
                Tag::Paragraph => {
                    current_spans.clear();
                }
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    code_block_lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code_block_content.clear();
                }
                Tag::Emphasis => { italic = true; }
                Tag::Strong => { bold = true; }
                Tag::Link { dest_url, .. } => {
                    link_url = Some(dest_url.to_string());
                }
                Tag::BlockQuote(_) => { in_blockquote = true; }
                Tag::List(start) => {
                    let ordered = start.is_some();
                    let idx = start.unwrap_or(0) as usize;
                    list_stack.push((ordered, idx));
                }
                Tag::Item => {
                    current_spans.clear();
                }
                Tag::Table(_alignments) => {
                    in_table = true;
                    table_headers.clear();
                    table_rows.clear();
                }
                Tag::TableHead => {
                    in_table_head = true;
                    current_row.clear();
                }
                Tag::TableRow => {
                    current_row.clear();
                }
                Tag::TableCell => {
                    current_spans.clear();
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    if let Some(level) = in_heading.take() {
                        let text = spans_to_plain(&current_spans);
                        elements.push(PreviewElement::Heading { level, text });
                        current_spans.clear();
                    }
                }
                TagEnd::Paragraph => {
                    if !current_spans.is_empty() {
                        if in_blockquote {
                            elements.push(PreviewElement::Blockquote { spans: current_spans.clone() });
                        } else {
                            elements.push(PreviewElement::Paragraph { spans: current_spans.clone() });
                        }
                        current_spans.clear();
                    }
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    elements.push(format_code_block(&code_block_lang, &code_block_content));
                }
                TagEnd::Emphasis => { italic = false; }
                TagEnd::Strong => { bold = false; }
                TagEnd::Link => { link_url = None; }
                TagEnd::BlockQuote(_) => { in_blockquote = false; }
                TagEnd::List(_) => { list_stack.pop(); }
                TagEnd::Table => {
                    in_table = false;
                    elements.push(PreviewElement::Table {
                        headers: table_headers.clone(),
                        rows: table_rows.clone(),
                    });
                    table_headers.clear();
                    table_rows.clear();
                }
                TagEnd::TableHead => {
                    in_table_head = false;
                    table_headers = current_row.clone();
                    current_row.clear();
                }
                TagEnd::TableRow => {
                    if !in_table_head {
                        table_rows.push(current_row.clone());
                    }
                    current_row.clear();
                }
                TagEnd::TableCell => {
                    current_row.push(current_spans.clone());
                    current_spans.clear();
                }
                TagEnd::Item => {
                    if !current_spans.is_empty() {
                        let (ordered, index) = list_stack.last().copied().unwrap_or((false, 0));
                        let depth = list_stack.len().saturating_sub(1) as u8;
                        elements.push(PreviewElement::ListItem {
                            depth,
                            ordered,
                            index,
                            spans: current_spans.clone(),
                        });
                        // Increment ordered list index
                        if let Some(last) = list_stack.last_mut() {
                            last.1 += 1;
                        }
                        current_spans.clear();
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_block_content.push_str(&text);
                } else {
                    current_spans.push(TextSpan {
                        text: text.to_string(),
                        bold,
                        italic,
                        code: in_code_span,
                        link_url: link_url.clone(),
                    });
                }
            }
            Event::Code(code) => {
                current_spans.push(TextSpan {
                    text: code.to_string(),
                    bold,
                    italic,
                    code: true,
                    link_url: None,
                });
            }
            Event::SoftBreak | Event::HardBreak => {
                current_spans.push(TextSpan {
                    text: " ".to_string(),
                    bold: false, italic: false, code: false, link_url: None,
                });
            }
            Event::Rule => {
                elements.push(PreviewElement::HorizontalRule);
            }
            _ => {}
        }
    }

    // Flush remaining spans
    if !current_spans.is_empty() {
        elements.push(PreviewElement::Paragraph { spans: current_spans });
    }

    elements
}

fn spans_to_plain(spans: &[TextSpan]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

fn detect_language(path: &str) -> String {
    match path.rsplit('.').next() {
        Some("rs") => "rust",
        Some("js") | Some("jsx") => "javascript",
        Some("ts") | Some("tsx") => "typescript",
        Some("py") => "python",
        Some("toml") => "toml",
        Some("json") => "json",
        Some("yaml") | Some("yml") => "yaml",
        Some("sh") | Some("bash") => "bash",
        Some("css") => "css",
        Some("html") | Some("xml") => "html",
        Some("go") => "go",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("hpp") | Some("cc") => "cpp",
        Some("java") | Some("kt") => "java",
        Some("rb") => "ruby",
        Some("lua") => "lua",
        Some("sql") => "sql",
        _ => "",
    }.to_string()
}

// ─── Syntax Highlighting ───────────────────────────────────────

/// A colored text token for syntax-highlighted code
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct SyntaxToken {
    text: String,
    color: u32,
}

/// Tokenize a line of code with syntax coloring.
/// Returns a list of colored tokens for the given language.
#[cfg(feature = "gpui")]
fn highlight_line(line: &str, lang: &str) -> Vec<SyntaxToken> {
    let keywords = language_keywords(lang);
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Line comment: // or #
        if (ch == '/' && i + 1 < len && chars[i + 1] == '/')
            || (ch == '#' && matches!(lang, "python" | "ruby" | "bash" | "yaml" | "toml"))
        {
            let rest: String = chars[i..].iter().collect();
            tokens.push(SyntaxToken { text: rest, color: 0x969896 }); // gray
            break;
        }

        // String: "..." or '...'
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut s = String::new();
            s.push(ch);
            i += 1;
            while i < len {
                s.push(chars[i]);
                if chars[i] == quote && (i == 0 || chars[i - 1] != '\\') {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(SyntaxToken { text: s, color: 0xb5bd68 }); // green
            continue;
        }

        // Backtick string (JS/Go)
        if ch == '`' && matches!(lang, "javascript" | "typescript" | "go") {
            let mut s = String::new();
            s.push(ch);
            i += 1;
            while i < len {
                s.push(chars[i]);
                if chars[i] == '`' { i += 1; break; }
                i += 1;
            }
            tokens.push(SyntaxToken { text: s, color: 0xb5bd68 }); // green
            continue;
        }

        // Number
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let mut s = String::new();
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_' || chars[i] == 'x') {
                s.push(chars[i]);
                i += 1;
            }
            tokens.push(SyntaxToken { text: s, color: 0xf0c674 }); // yellow
            continue;
        }

        // Word (keyword / identifier / type)
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut word = String::new();
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                word.push(chars[i]);
                i += 1;
            }
            let color = if keywords.contains(&word.as_str()) {
                0x81a2be // blue — keyword
            } else if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                && word.len() > 1
                && !word.chars().all(|c| c.is_uppercase() || c == '_')
            {
                0xf0c674 // yellow — PascalCase type
            } else if matches!(lang, "rust" | "go" | "python")
                && matches!(word.as_str(), "self" | "Self" | "true" | "false" | "None" | "nil" | "null")
            {
                0xcc6666 // red — special values
            } else {
                0xc5c8c6 // default text
            };
            tokens.push(SyntaxToken { text: word, color });
            continue;
        }

        // Macro/attribute: @ or #[
        if ch == '@' || (ch == '#' && i + 1 < len && chars[i + 1] == '[') {
            let mut s = String::new();
            while i < len && chars[i] != '\n' && chars[i] != ' ' && chars[i] != '(' {
                s.push(chars[i]);
                i += 1;
                if ch == '#' && s.ends_with(']') { break; }
            }
            tokens.push(SyntaxToken { text: s, color: 0xb294bb }); // purple
            continue;
        }

        // Operators and punctuation
        tokens.push(SyntaxToken { text: ch.to_string(), color: 0x969896 });
        i += 1;
    }

    if tokens.is_empty() {
        tokens.push(SyntaxToken { text: " ".to_string(), color: 0xc5c8c6 });
    }
    tokens
}

/// Get keywords for a language
fn language_keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => &[
            "fn", "let", "mut", "const", "static", "struct", "enum", "impl", "trait",
            "type", "pub", "crate", "mod", "use", "super", "as", "if", "else", "match",
            "for", "while", "loop", "break", "continue", "return", "where", "in",
            "ref", "move", "async", "await", "unsafe", "dyn", "Box", "Vec", "Option",
            "Result", "Some", "None", "Ok", "Err", "String", "str", "bool", "usize",
            "i32", "u32", "i64", "u64", "f32", "f64", "u8", "i8", "u16", "i16",
        ],
        "javascript" | "typescript" => &[
            "function", "const", "let", "var", "if", "else", "for", "while", "do",
            "return", "class", "extends", "new", "this", "super", "import", "export",
            "from", "default", "async", "await", "try", "catch", "finally", "throw",
            "switch", "case", "break", "continue", "typeof", "instanceof", "in", "of",
            "interface", "type", "enum", "implements", "abstract", "readonly", "private",
            "public", "protected", "static", "void", "never", "any", "string", "number",
            "boolean", "undefined", "null", "true", "false", "yield", "delete",
        ],
        "python" => &[
            "def", "class", "if", "elif", "else", "for", "while", "return", "import",
            "from", "as", "try", "except", "finally", "raise", "with", "pass", "break",
            "continue", "lambda", "yield", "assert", "global", "nonlocal", "del", "and",
            "or", "not", "is", "in", "True", "False", "None", "async", "await", "print",
        ],
        "go" => &[
            "func", "package", "import", "var", "const", "type", "struct", "interface",
            "map", "chan", "if", "else", "for", "range", "switch", "case", "default",
            "return", "break", "continue", "go", "defer", "select", "fallthrough",
            "true", "false", "nil", "make", "new", "len", "cap", "append", "error",
        ],
        "java" | "cpp" | "c" => &[
            "class", "public", "private", "protected", "static", "void", "int", "long",
            "float", "double", "char", "boolean", "byte", "short", "if", "else", "for",
            "while", "do", "switch", "case", "break", "continue", "return", "new",
            "try", "catch", "finally", "throw", "throws", "import", "package", "extends",
            "implements", "interface", "abstract", "final", "this", "super", "null",
            "true", "false", "const", "unsigned", "signed", "struct", "enum", "typedef",
            "include", "define", "ifdef", "ifndef", "endif", "namespace", "using",
            "template", "typename", "auto", "virtual", "override",
        ],
        "sql" => &[
            "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET",
            "DELETE", "CREATE", "TABLE", "ALTER", "DROP", "INDEX", "JOIN", "LEFT",
            "RIGHT", "INNER", "OUTER", "ON", "AND", "OR", "NOT", "IN", "NULL",
            "IS", "AS", "ORDER", "BY", "GROUP", "HAVING", "LIMIT", "OFFSET",
            "UNION", "ALL", "DISTINCT", "EXISTS", "BETWEEN", "LIKE", "PRIMARY", "KEY",
            "FOREIGN", "REFERENCES", "DEFAULT", "CASCADE", "CONSTRAINT", "CHECK",
            "select", "from", "where", "insert", "into", "values", "update", "set",
            "delete", "create", "table", "alter", "drop", "join", "left", "right",
            "inner", "outer", "on", "and", "or", "not", "in", "null", "is", "as",
            "order", "by", "group", "having", "limit", "offset",
        ],
        "ruby" => &[
            "def", "end", "class", "module", "if", "elsif", "else", "unless", "while",
            "until", "for", "do", "begin", "rescue", "ensure", "raise", "return",
            "yield", "block_given?", "require", "include", "extend", "attr_accessor",
            "attr_reader", "attr_writer", "self", "super", "true", "false", "nil",
            "and", "or", "not", "in", "then", "when", "case", "puts", "print",
        ],
        "lua" => &[
            "function", "local", "if", "then", "else", "elseif", "end", "for", "while",
            "do", "repeat", "until", "return", "break", "in", "and", "or", "not",
            "true", "false", "nil", "require",
        ],
        _ => &[
            "if", "else", "for", "while", "return", "function", "class", "import",
            "true", "false", "null", "nil", "None", "const", "let", "var",
        ],
    }
}

/// File type icon for header display
#[cfg(feature = "gpui")]
fn file_type_icon(name: &str) -> &'static str {
    if name.ends_with(".md") || name.ends_with(".markdown") { "MARKDOWN" }
    else if name.ends_with(".rs") { "RUST" }
    else if name.ends_with(".js") || name.ends_with(".jsx") { "JS" }
    else if name.ends_with(".ts") || name.ends_with(".tsx") { "TS" }
    else if name.ends_with(".py") { "PYTHON" }
    else if name.ends_with(".json") { "JSON" }
    else if name.ends_with(".toml") { "TOML" }
    else if name.ends_with(".yaml") || name.ends_with(".yml") { "YAML" }
    else if name.ends_with(".html") { "HTML" }
    else if name.ends_with(".css") { "CSS" }
    else if name.ends_with(".go") { "GO" }
    else if name.ends_with(".sh") || name.ends_with(".bash") { "SHELL" }
    else { "FILE" }
}

// ─── GPUI Rendering ────────────────────────────────────────────

#[cfg(feature = "gpui")]
pub fn render_preview_panel(
    state: &PreviewState,
    cx: &mut gpui::Context<GpuiShellView>,
) -> AnyElement {
    div()
        .id("preview-panel")
        .flex()
        .flex_row()
        .h_full()
        .overflow_hidden()
        // Resize handle (left edge)
        .child(
            div()
                .id("preview-resize-handle")
                .group("preview-handle")
                .w(px(4.0))
                .h_full()
                .flex_shrink_0()
                .cursor_col_resize()
                .child(
                    div()
                        .w(px(1.0))
                        .h_full()
                        .bg(rgb(0x282a2e))
                        .group_hover("preview-handle", |d| d.w(px(2.0)).bg(rgb(0x81a2be)))
                )
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _w, _cx| {
                    if let Some(ref state) = this.preview_state {
                        this.preview_drag_start = Some(
                            (event.position.x.as_f32(), state.width)
                        );
                    }
                }))
        )
        // Content column
        .child(
        div()
        .flex_1()
        .flex()
        .flex_col()
        .h_full()
        .bg(rgb(0x1d1f21))
        .overflow_hidden()
        // Header bar
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py(px(6.0))
                .bg(rgb(0x181a1e))
                .border_b_1()
                .border_color(rgb(0x282a2e))
                .flex_shrink_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div().text_xs().text_color(rgb(0x81a2be))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(file_type_icon(&state.file_name))
                        )
                        .child(
                            div().text_xs().text_color(rgb(0xc5c8c6))
                                .font_weight(FontWeight::MEDIUM)
                                .child(state.file_name.clone())
                        )
                        .child(
                            div().text_xs().text_color(rgb(0x969896))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(format!("  {}", state.file_path))
                        )
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .flex_shrink_0()
                        // Copy button
                        .child(
                            div()
                                .id("preview-copy")
                                .text_xs()
                                .text_color(rgb(0x969896))
                                .px(px(5.0))
                                .py(px(2.0))
                                .rounded(px(3.0))
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(0xb5bd68)).bg(rgb(0x282a2e)))
                                .child("Copy")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(ref state) = this.preview_state {
                                        if let Ok(content) = std::fs::read_to_string(&state.file_path) {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(content));
                                        }
                                    }
                                }))
                        )
                        // Keyboard hint
                        .child(
                            div().text_xs().text_color(rgb(0x969896)).child("Esc")
                        )
                        // Close button
                        .child(
                            div()
                                .id("preview-close")
                                .text_xs()
                                .text_color(rgb(0x969896))
                                .px(px(5.0))
                                .py(px(2.0))
                                .rounded(px(3.0))
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(0xcc6666)).bg(rgb(0x282a2e)))
                                .child("✕")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preview_state = None;
                                    cx.notify();
                                }))
                        )
                )
        )
        // Content area (scrollable)
        .child(
            div()
                .id(gpui::ElementId::Name("preview-content".into()))
                .flex_1()
                .overflow_y_scroll()
                .px(px(20.0))
                .py(px(16.0))
                .children(state.elements.iter().map(|el| render_element(el)))
        )
        ) // end content column
        .into_any_element()
}

#[cfg(feature = "gpui")]
fn render_element(el: &PreviewElement) -> AnyElement {
    match el {
        PreviewElement::Heading { level, text } => {
            let (size, color, mt) = match level {
                1 => (18.0, 0x81a2be, 16.0),  // blue, large
                2 => (16.0, 0xb5bd68, 14.0),  // green
                3 => (14.0, 0xf0c674, 12.0),  // yellow
                _ => (13.0, 0xc5c8c6, 10.0),  // white
            };
            div()
                .mt(px(mt))
                .mb(px(6.0))
                .child(
                    div()
                        .text_size(px(size))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(color))
                        .child(text.clone())
                )
                .when(*level <= 2, |d| {
                    d.child(
                        div().w_full().h(px(1.0)).mt(px(4.0)).bg(rgb(0x282a2e))
                    )
                })
                .into_any_element()
        }

        PreviewElement::Paragraph { spans } => {
            div()
                .mb(px(10.0))
                .child(render_spans(spans))
                .into_any_element()
        }

        PreviewElement::CodeBlock { language, formatted_lines, total_lines } => {
            let lang_label = if language.is_empty() { None } else { Some(language.clone()) };
            let truncated = formatted_lines.len() < *total_lines;

            div()
                .mb(px(10.0))
                .w_full()
                .bg(rgb(0x141618))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(0x282a2e))
                .overflow_hidden()
                .when_some(lang_label, |d, lang| {
                    d.child(
                        div()
                            .flex()
                            .justify_between()
                            .px_3()
                            .py(px(3.0))
                            .bg(rgb(0x181a1e))
                            .border_b_1()
                            .border_color(rgb(0x282a2e))
                            .text_xs()
                            .text_color(rgb(0x969896))
                            .child(lang)
                            .child(format!("{} lines", total_lines))
                    )
                })
                .child(
                    div()
                        .px_2()
                        .py(px(4.0))
                        .text_xs()
                        // Pre-formatted lines: zero computation per frame
                        .children(formatted_lines.iter().map(|(text, color)| {
                            div()
                                .text_color(rgb(*color))
                                .whitespace_nowrap()
                                .child(text.clone())
                                .into_any_element()
                        }))
                )
                .when(truncated, |d| {
                    d.child(
                        div()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(rgb(0x969896))
                            .bg(rgb(0x181a1e))
                            .border_t_1()
                            .border_color(rgb(0x282a2e))
                            .child(format!("... {} more lines", total_lines - formatted_lines.len()))
                    )
                })
                .into_any_element()
        }

        PreviewElement::ListItem { depth, ordered, index, spans } => {
            let indent = (*depth as f32 + 1.0) * 16.0;
            let bullet = if *ordered {
                format!("{}.", index + 1)
            } else {
                match depth {
                    0 => "•".to_string(),
                    1 => "◦".to_string(),
                    _ => "▪".to_string(),
                }
            };
            div()
                .mb(px(3.0))
                .pl(px(indent))
                .flex()
                .gap(px(6.0))
                .child(
                    div().text_sm().text_color(rgb(0x969896)).w(px(16.0))
                        .flex_shrink_0()
                        .child(bullet)
                )
                .child(render_spans(spans))
                .into_any_element()
        }

        PreviewElement::HorizontalRule => {
            div()
                .my_3()
                .w_full()
                .h(px(1.0))
                .bg(rgb(0x373b41))
                .into_any_element()
        }

        PreviewElement::Blockquote { spans } => {
            div()
                .mb(px(8.0))
                .pl(px(12.0))
                .border_l_2()
                .border_color(rgb(0x969896))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xb4b7b4))
                        .italic()
                        .child(render_spans(spans))
                )
                .into_any_element()
        }

        PreviewElement::Table { headers, rows } => {
            let col_count = headers.len();
            div()
                .mb(px(12.0))
                .w_full()
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(0x373b41))
                .overflow_hidden()
                // Header row
                .child(
                    div()
                        .flex()
                        .bg(rgb(0x282a2e))
                        .border_b_1()
                        .border_color(rgb(0x373b41))
                        .children(headers.iter().map(|cell_spans| {
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(5.0))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xc5c8c6))
                                .border_r_1()
                                .border_color(rgb(0x373b41))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(render_spans(cell_spans))
                                .into_any_element()
                        }))
                )
                // Data rows
                .children(rows.iter().enumerate().map(|(row_idx, row)| {
                    let row_bg = if row_idx % 2 == 0 { rgb(0x1d1f21) } else { rgb(0x222426) };
                    div()
                        .flex()
                        .bg(row_bg)
                        .when(row_idx + 1 < rows.len(), |d| {
                            d.border_b_1().border_color(rgb(0x282a2e))
                        })
                        .children(row.iter().map(|cell_spans| {
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(4.0))
                                .text_xs()
                                .text_color(rgb(0xc5c8c6))
                                .border_r_1()
                                .border_color(rgb(0x282a2e))
                                .overflow_hidden()
                                .child(render_spans(cell_spans))
                                .into_any_element()
                        }))
                        // Pad missing cells if row has fewer columns than header
                        .children((row.len()..col_count).map(|_| {
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(4.0))
                                .into_any_element()
                        }))
                        .into_any_element()
                }))
                .into_any_element()
        }

        PreviewElement::BlankLine => {
            div().h(px(8.0)).into_any_element()
        }
    }
}

#[cfg(feature = "gpui")]
fn render_spans(spans: &[TextSpan]) -> AnyElement {
    // Merge adjacent plain-text spans (same style) to reduce div count
    let mut merged: Vec<&TextSpan> = Vec::new();
    // For simplicity, just render each unique-styled span as one div
    // but concatenate adjacent spans with identical styling
    div()
        .text_sm()
        .text_color(rgb(0xc5c8c6))
        .children(spans.iter().map(|span| {
            let mut d = div().child(span.text.clone());

            if span.bold {
                d = d.font_weight(FontWeight::BOLD);
            }
            if span.italic {
                d = d.italic();
            }
            if span.code {
                d = d
                    .bg(rgb(0x282a2e))
                    .rounded(px(3.0))
                    .px(px(4.0))
                    .py(px(1.0))
                    .text_xs()
                    .text_color(rgb(0xcc6666));
            }
            if span.link_url.is_some() {
                d = d.text_color(rgb(0x81a2be));
            }

            // Only use inline display for styled spans, plain text flows naturally
            if span.bold || span.italic || span.code || span.link_url.is_some() {
                d = d.flex_shrink_0();
            }

            d.into_any_element()
        }))
        .into_any_element()
}

// ─── File Picker ───────────────────────────────────────────────

#[cfg(feature = "gpui")]
impl FilePickerState {
    pub fn new(cwd: Option<String>) -> Self {
        // Scan once on open, cache the full file list
        let all_files = Self::scan_all_files(cwd);
        let matches = Self::filter_files(&all_files, "", 20);
        Self {
            query: String::new(),
            all_files,
            matches,
            selected_index: 0,
        }
    }

    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        // Filter cached file list — no filesystem access
        self.matches = Self::filter_files(&self.all_files, query, 20);
        self.selected_index = 0;
    }

    /// Filter cached file list by query (fast, in-memory only)
    fn filter_files(all_files: &[String], query: &str, max: usize) -> Vec<String> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<String> = if query.is_empty() {
            all_files.iter().take(max * 3).cloned().collect()
        } else {
            all_files.iter()
                .filter(|p| p.to_lowercase().contains(&query_lower))
                .take(max * 3)
                .cloned()
                .collect()
        };
        // Sort: .md files first, then by path
        results.sort_by(|a, b| {
            let a_md = a.ends_with(".md") || a.ends_with(".markdown");
            let b_md = b.ends_with(".md") || b.ends_with(".markdown");
            b_md.cmp(&a_md).then_with(|| a.cmp(b))
        });
        results.truncate(max);
        results
    }

    /// Scan filesystem once, return all previewable files
    fn scan_all_files(cwd: Option<String>) -> Vec<String> {
        let cwd = cwd.map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let mut results = Vec::new();
        Self::walk_dir(&cwd, &cwd, &mut results, 200, 0);
        results
    }

    fn walk_dir(
        base: &std::path::Path,
        dir: &std::path::Path,
        results: &mut Vec<String>,
        max: usize,
        depth: usize,
    ) {
        if results.len() >= max || depth > 4 { return; }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            if results.len() >= max { break; }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden dirs and common ignore dirs
            if name.starts_with('.') || name == "node_modules" || name == "target"
                || name == "third_party" || name == "__pycache__" || name == "dist"
                || name == "build" || name == "vendor" {
                continue;
            }

            if path.is_dir() {
                Self::walk_dir(base, &path, results, max, depth + 1);
            } else {
                // Match previewable files
                let is_previewable = matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("md" | "markdown" | "txt" | "rs" | "js" | "ts" | "py" | "toml"
                        | "json" | "yaml" | "yml" | "sh" | "bash" | "css" | "html"
                        | "tsx" | "jsx" | "go" | "c" | "cpp" | "h" | "hpp" | "java"
                        | "rb" | "php" | "swift" | "kt" | "lua" | "vim" | "sql"
                        | "xml" | "ini" | "cfg" | "conf" | "log")
                );
                if !is_previewable { continue; }

                let rel_path = path.strip_prefix(base)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string_lossy().to_string());

                results.push(rel_path);
            }
        }
    }
}

/// Render the file picker overlay
#[cfg(feature = "gpui")]
pub fn render_file_picker(
    picker: &FilePickerState,
    cx: &mut gpui::Context<GpuiShellView>,
) -> AnyElement {
    let query_display = if picker.query.is_empty() {
        "▎ Search files...".to_string()
    } else {
        format!("{}▎", picker.query)
    };
    let query_color = if picker.query.is_empty() { rgb(0x969896) } else { rgb(0xc5c8c6) };

    div()
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .pt(px(80.0))
        // Backdrop
        .child(
            div()
                .id("file-picker-backdrop")
                .absolute()
                .inset_0()
                .bg(gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.4 })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.file_picker = None;
                    cx.notify();
                }))
        )
        // Picker panel
        .child(
            div()
                .id("file-picker-panel")
                .w(px(500.0))
                .max_h(px(400.0))
                .bg(rgb(0x1d1f21))
                .border_1()
                .border_color(rgb(0x373b41))
                .rounded(px(8.0))
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.focus_handle.focus(window, cx);
                }))
                // Search input
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(0x282a2e))
                        .child(
                            div().text_xs().text_color(rgb(0x81a2be)).child("🔍")
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(query_color)
                                .child(query_display)
                        )
                )
                // Results
                .child(
                    div()
                        .id(gpui::ElementId::Name("fp-results".into()))
                        .flex_1()
                        .overflow_y_scroll()
                        .children(
                            if picker.matches.is_empty() {
                                vec![
                                    div()
                                        .px_3()
                                        .py_2()
                                        .text_xs()
                                        .text_color(rgb(0x969896))
                                        .child("No files found")
                                        .into_any_element()
                                ]
                            } else {
                                picker.matches.iter().enumerate().map(|(i, path)| {
                                    let is_selected = i == picker.selected_index;
                                    let bg = if is_selected { rgb(0x282a2e) } else { rgb(0x1d1f21) };
                                    let text_c = if is_selected { rgb(0xc5c8c6) } else { rgb(0x969896) };

                                    // Highlight .md files with an icon
                                    let icon = if path.ends_with(".md") { "📄 " }
                                        else if path.ends_with(".rs") { "🦀 " }
                                        else if path.ends_with(".json") || path.ends_with(".toml") { "⚙️ " }
                                        else { "   " };

                                    div()
                                        .id(gpui::ElementId::Name(format!("fp-{}", i).into()))
                                        .flex()
                                        .items_center()
                                        .px_3()
                                        .py(px(5.0))
                                        .bg(bg)
                                        .text_xs()
                                        .text_color(text_c)
                                        .cursor_pointer()
                                        .hover(|d| d.bg(rgb(0x282a2e)))
                                        .when(is_selected, |d| d.border_l_2().border_color(rgb(0x81a2be)))
                                        .child(icon.to_string())
                                        .child(path.clone())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open_preview_from_picker(i);
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                }).collect()
                            }
                        )
                )
                // Footer
                .child(
                    div()
                        .px_3()
                        .py(px(4.0))
                        .border_t_1()
                        .border_color(rgb(0x282a2e))
                        .text_xs()
                        .text_color(rgb(0x969896))
                        .child("Enter: preview  ↑↓: navigate  Esc: close")
                )
        )
        .into_any_element()
}
