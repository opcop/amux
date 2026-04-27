//! Built-in file preview — renders Markdown, code, and plain text
//! using GPUI native elements (no WebView).

#[cfg(feature = "gpui")]
use gpui::{
    rgb, px, div, list, prelude::*, uniform_list, AnyElement, FontWeight, IntoElement,
    ParentElement, Styled,
};

#[cfg(feature = "gpui")]
use std::sync::Arc;

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
    /// Index of every Markdown heading in `elements`, precomputed at
    /// load time. Used for TOC overlay (`o`), heading jump (`[` / `]`),
    /// and fuzzy heading search (`:`). Empty for non-Markdown files or
    /// markdown with no headings.
    pub headings: Vec<HeadingEntry>,
    /// Monotonic sequence number bumped on every load/reload. The
    /// text-selection subsystem captures this in
    /// `PreviewSelectionState.generation` on mouse-down; a mismatch
    /// on any render tick means the document was swapped (auto-
    /// reload, placeholder → real parse, etc.) and the stored
    /// selection may no longer map to valid text. The render-loop
    /// invalidator drops such selections before they cause a
    /// mis-copy. See `plans/preview-text-selection-spec.md` §3
    /// "Auto-reload interaction".
    pub generation: u64,
}

/// Global monotonic counter for `PreviewState.generation`. A single
/// atomic covers every preview state in every workspace — uniqueness
/// across reload / tab-switch / path-reuse is what matters, not
/// per-path sequencing. An atomic `fetch_add` is ~1 ns and runs on
/// the UI thread path anyway, so there's no hot-spot risk.
#[cfg(feature = "gpui")]
pub(crate) fn next_preview_generation() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A single heading in the preview, with cached section bounds and
/// content text. Mirrors mdterm's `TocEntry` structure: `section_end`
/// is the **exclusive** index of the first element that ends this
/// heading's section (next same-or-higher-level heading, or
/// `elements.len()` if this is the last one). That range is what a
/// future "copy section" shortcut copies.
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub struct HeadingEntry {
    pub level: u8,
    pub text: String,
    /// Index into `PreviewState.elements` where the `Heading` lives.
    pub element_idx: usize,
    /// Exclusive end — the element index where the next same-level-or-
    /// higher heading starts, or `elements.len()` for the last one.
    pub section_end_idx: usize,
    /// Concatenated plain text of every element in `[element_idx ..
    /// section_end_idx]`. Used for fuzzy match and "copy section".
    /// Computed once at load time so fuzzy filtering stays cheap per
    /// keystroke.
    pub content_text: String,
}

/// A renderable element in the preview
#[cfg(feature = "gpui")]
#[derive(Clone, Debug)]
pub enum PreviewElement {
    Heading { level: u8, text: String },
    Paragraph { spans: Vec<TextSpan> },
    CodeBlock {
        language: String,
        /// Pre-formatted lines: (line_number, code_text, dominant_color). Computed once at load time.
        formatted_lines: Vec<(String, String, u32)>,
        total_lines: usize,
    },
    ListItem { depth: u8, ordered: bool, index: usize, spans: Vec<TextSpan> },
    HorizontalRule,
    Blockquote { spans: Vec<TextSpan> },
    Table { headers: Vec<Vec<TextSpan>>, rows: Vec<Vec<Vec<TextSpan>>> },
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
    /// All previewable files scanned under `base_dir`, paired with
    /// their last-modified time. Keeping the mtime here means
    /// `filter_files` can sort by recency without re-`stat`'ing
    /// every file on every keystroke.
    all_files: Vec<(String, std::time::SystemTime)>,
    /// Filtered matches for current query
    pub matches: Vec<String>,
    pub selected_index: usize,
    /// The base directory these files are relative to
    pub base_dir: Option<String>,
}

// ─── Markdown Parsing ──────────────────────────────────────────

#[cfg(feature = "gpui")]
impl PreviewState {
    /// Load and parse a file for preview
    pub fn load(file_path: &str) -> Option<Self> {
        let metadata = std::fs::metadata(file_path).ok()?;
        let is_markdown = file_path.ends_with(".md") || file_path.ends_with(".markdown");

        // Split caps by file kind:
        //
        // * Markdown caps at 20 MB because pulldown-cmark produces a
        //   heterogeneous element tree (headings, paragraphs, lists,
        //   tables) that has to be fully realized in the DOM — GPUI
        //   layout is O(N) over elements, so unbounded markdown would
        //   stall on multi-MB specs. 20 MB covers any real-world doc.
        //
        // * Code (everything else) caps at 100 MB only as a sanity
        //   guard against someone accidentally previewing a binary
        //   dump as text. The renderer uses `uniform_list` virtual
        //   scrolling for pure-code files, so the per-frame cost
        //   scales with viewport size, not file size — a 1 GB log
        //   would render fine, but reading 1 GB into memory via
        //   `read_to_string` would still stall the load thread and
        //   blow up RSS. 100 MB is the compromise.
        let size_cap: u64 = if is_markdown { 20 * 1024 * 1024 } else { 100 * 1024 * 1024 };
        if metadata.len() > size_cap {
            let file_name = std::path::Path::new(file_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file_path.to_string());
            return Some(Self {
                file_path: file_path.to_string(),
                file_name,
                elements: vec![PreviewElement::Paragraph {
                    spans: vec![TextSpan {
                        text: format!(
                            "File too large to preview ({:.1} MB — cap {} MB)",
                            metadata.len() as f64 / 1024.0 / 1024.0,
                            size_cap / 1024 / 1024,
                        ),
                        bold: false,
                        italic: true,
                        code: false,
                        link_url: None,
                    }],
                }],
                headings: Vec::new(),
                generation: next_preview_generation(),
            });
        }

        let content = std::fs::read_to_string(file_path).ok()?;
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());

        // Catch panics from markdown/syntax parsing to prevent crashes
        let elements = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if is_markdown {
                parse_markdown(&content)
            } else {
                let lang = detect_language(file_path);
                vec![format_code_block(&lang, &content)]
            }
        })).unwrap_or_else(|_| {
            eprintln!("[amux-preview] panic while parsing: {}", file_path);
            // Fallback: plain-text dump. No truncation — the renderer
            // virtualizes this path via `uniform_list`, so a 100k-line
            // log is just as cheap to display as a 10-line one.
            let all_lines: Vec<&str> = content.lines().collect();
            let total = all_lines.len();
            let gutter_w = gutter_width_for(total);
            vec![PreviewElement::CodeBlock {
                language: "text".to_string(),
                formatted_lines: all_lines.iter().enumerate()
                    .map(|(i, l)| (format!("{:>width$}", i + 1, width = gutter_w), l.to_string(), 0xc5c8c6))
                    .collect(),
                total_lines: total,
            }]
        });

        let headings = build_headings_index(&elements);
        Some(Self {
            file_path: file_path.to_string(),
            file_name,
            elements,
            headings,
            generation: next_preview_generation(),
        })
    }

    /// Synchronous placeholder inserted while the real `load` runs on a
    /// background thread. Exists so clicking a path in the terminal
    /// produces an immediate tab+panel instead of a frame stall while
    /// pulldown-cmark / syntax highlighting chew through the file.
    pub fn loading_placeholder(file_path: &str) -> Self {
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());
        Self {
            file_path: file_path.to_string(),
            file_name,
            elements: vec![PreviewElement::Paragraph {
                spans: vec![TextSpan {
                    text: "Loading…".to_string(),
                    bold: false,
                    italic: true,
                    code: false,
                    link_url: None,
                }],
            }],
            headings: Vec::new(),
            generation: next_preview_generation(),
        }
    }
}

/// Scan `elements` for `Heading` variants and produce a `HeadingEntry`
/// for each with its section bounds + concatenated content text.
/// Runs once per load — O(N) in element count, cheap even on 20 MB
/// markdown docs.
#[cfg(feature = "gpui")]
fn build_headings_index(elements: &[PreviewElement]) -> Vec<HeadingEntry> {
    // Pass 1: collect every heading's (element_idx, level, text).
    let mut entries: Vec<HeadingEntry> = Vec::new();
    for (i, el) in elements.iter().enumerate() {
        if let PreviewElement::Heading { level, text } = el {
            entries.push(HeadingEntry {
                level: *level,
                text: text.clone(),
                element_idx: i,
                section_end_idx: 0, // filled in pass 2
                content_text: String::new(), // filled in pass 3
            });
        }
    }
    // Pass 2: section_end_idx. Walk right-to-left so each entry can
    // look up its successor. A section ends at the next heading whose
    // level is ≤ this one's (equal or higher in hierarchy). If none
    // exists, the section runs to the end of the document.
    let total = elements.len();
    for i in (0..entries.len()).rev() {
        let lvl = entries[i].level;
        let end = entries[i + 1..]
            .iter()
            .find(|e| e.level <= lvl)
            .map(|e| e.element_idx)
            .unwrap_or(total);
        entries[i].section_end_idx = end;
    }
    // Pass 3: content_text for fuzzy matching and future copy-section.
    // Concatenate plain text of every element in the section range.
    for i in 0..entries.len() {
        let s = entries[i].element_idx;
        let e = entries[i].section_end_idx;
        entries[i].content_text = elements[s..e]
            .iter()
            .map(element_plain_text)
            .collect::<Vec<_>>()
            .join("\n");
    }
    entries
}

/// Strip styling and return the plain-text content of an element.
/// Used by `build_headings_index` to populate `content_text`. Best-
/// effort — tables flatten to row-joined text, code blocks return
/// their joined lines. The point is readable text for fuzzy match,
/// not byte-perfect round-trip.
#[cfg(feature = "gpui")]
fn element_plain_text(el: &PreviewElement) -> String {
    match el {
        PreviewElement::Heading { text, .. } => text.clone(),
        PreviewElement::Paragraph { spans } | PreviewElement::Blockquote { spans } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        PreviewElement::ListItem { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        PreviewElement::CodeBlock { formatted_lines, .. } => formatted_lines
            .iter()
            .map(|(_, t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        PreviewElement::Table { headers, rows } => {
            let mut lines = Vec::new();
            lines.push(
                headers
                    .iter()
                    .map(|cell| cell.iter().map(|s| s.text.as_str()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
            for row in rows {
                lines.push(
                    row.iter()
                        .map(|cell| cell.iter().map(|s| s.text.as_str()).collect::<String>())
                        .collect::<Vec<_>>()
                        .join(" | "),
                );
            }
            lines.join("\n")
        }
        PreviewElement::HorizontalRule => String::new(),
    }
}

/// Gutter column width (character count) for a given total line count.
/// Small files get a 2-char gutter, 10M-line files get 8. Shared between
/// the primary format path and the panic fallback so both agree on
/// column alignment.
#[cfg(feature = "gpui")]
fn gutter_width_for(total: usize) -> usize {
    match total {
        0..=99 => 2,
        100..=999 => 3,
        1_000..=9_999 => 4,
        10_000..=99_999 => 5,
        100_000..=999_999 => 6,
        1_000_000..=9_999_999 => 7,
        _ => 8,
    }
}

/// Pre-format a code block: highlight + format into (line_num, text, color)
/// rows. Processes every line in the file — the renderer uses
/// `uniform_list` virtual scrolling in the pure-code path, so the cost
/// of creating the per-line vec is linear in file size but the render
/// cost per frame stays O(viewport).
#[cfg(feature = "gpui")]
fn format_code_block(language: &str, code: &str) -> PreviewElement {
    let all_lines: Vec<&str> = code.lines().collect();
    let total = all_lines.len();
    let gutter_w = gutter_width_for(total);
    let formatted: Vec<(String, String, u32)> = all_lines.iter().enumerate().map(|(i, line)| {
        let tokens = highlight_line(line, language);
        let line_num = format!("{:>width$}", i + 1, width = gutter_w);
        let code_text: String = tokens.iter().map(|t| t.text.as_str()).collect();
        let color = tokens.iter()
            .find(|t| t.color != 0xc5c8c6 && t.color != 0x969896)
            .map(|t| t.color)
            .unwrap_or(0xc5c8c6);
        (line_num, code_text, color)
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
    let mut link_url: Option<String> = None;
    let mut in_heading: Option<u8> = None;
    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut code_block_content = String::new();
    let mut in_blockquote = false;
    let mut list_stack: Vec<(bool, usize)> = Vec::new(); // (ordered, next_index)

    // Table state
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
                        CodeBlockKind::Fenced(lang) => lang.to_lowercase(),
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
                        code: false,
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
    content_w: f32,
    content_h: f32,
    preview_search: Option<&crate::preview_search::PreviewSearchState>,
    scroll_handle: gpui::UniformListScrollHandle,
    list_state: Option<gpui::ListState>,
    toc: Option<&crate::preview_toc::TocPickerState>,
    selection_ctx: Option<crate::preview_selection::SelectionRenderCtx>,
    cx: &mut gpui::Context<GpuiShellView>,
) -> AnyElement {
    let copy_path = state.file_path.clone();
    // Only render the search bar / highlight when the search state
    // belongs to *this* preview — stale state from a previous preview
    // tab never bleeds through (see PreviewSearchState::path for the
    // rationale). We take the state by reference from the caller
    // instead of re-reading the view entity here: reading the entity
    // during `Render::render` double-leases it and panics.
    let search_for_this = preview_search
        .filter(|s| s.path == state.file_path)
        .cloned();
    let highlight_line = search_for_this
        .as_ref()
        .and_then(|s| s.current_line());
    // Likewise, the TOC overlay only paints if its stored path
    // matches this preview. Switching tabs away and back would
    // otherwise pop the overlay up on the wrong document.
    let toc_for_this = toc.filter(|s| s.path == state.file_path).cloned();
    div()
        .id("preview-panel")
        .flex()
        .flex_col()
        .w(px(content_w))
        .h(px(content_h))
        .overflow_hidden()
        .child(
        div()
        .flex_1()
        .flex()
        .flex_col()
        .h_full()
        .bg(rgb(crate::theme::SURFACE))
        .overflow_hidden()
        // Header bar
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py(px(6.0))
                .bg(rgb(crate::theme::SURFACE_DIM))
                .border_b_1()
                .border_color(rgb(crate::theme::SURFACE_RAISED))
                .flex_shrink_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div().text_xs().text_color(rgb(crate::theme::ACCENT))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(file_type_icon(&state.file_name))
                        )
                        .child(
                            div().text_xs().text_color(rgb(crate::theme::TEXT))
                                .font_weight(FontWeight::MEDIUM)
                                .child(state.file_name.clone())
                        )
                        .child(
                            div().text_xs().text_color(rgb(crate::theme::TEXT_DIM))
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
                        // TOC button — visible only when the doc has
                        // headings. Mirrors pressing `o`.
                        .when(!state.headings.is_empty(), |d| {
                            d.child(
                                div()
                                    .id("preview-toc-btn")
                                    .text_xs()
                                    .text_color(rgb(crate::theme::TEXT_DIM))
                                    .px(px(5.0))
                                    .py(px(2.0))
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(rgb(crate::theme::ACCENT)).bg(rgb(crate::theme::SURFACE_RAISED)))
                                    .child("TOC")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.preview_toc_open(cx);
                                    }))
                            )
                        })
                        // Find button — only for code-file previews
                        // where the search path is wired. Clicking it
                        // opens the same `/` search bar.
                        .when(
                            matches!(state.elements.as_slice(), [PreviewElement::CodeBlock { .. }]),
                            |d| {
                                d.child(
                                    div()
                                        .id("preview-find-btn")
                                        .text_xs()
                                        .text_color(rgb(crate::theme::TEXT_DIM))
                                        .px(px(5.0))
                                        .py(px(2.0))
                                        .rounded(px(3.0))
                                        .cursor_pointer()
                                        .hover(|d| d.text_color(rgb(crate::theme::ACCENT)).bg(rgb(crate::theme::SURFACE_RAISED)))
                                        .child("Find")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.preview_search_open(cx);
                                        }))
                                )
                            },
                        )
                        // Copy button
                        .child(
                            div()
                                .id("preview-copy")
                                .text_xs()
                                .text_color(rgb(crate::theme::TEXT_DIM))
                                .px(px(5.0))
                                .py(px(2.0))
                                .rounded(px(3.0))
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(crate::theme::SUCCESS)).bg(rgb(crate::theme::SURFACE_RAISED)))
                                .child("Copy")
                                .on_click(cx.listener(move |_this, _, _, cx| {
                                    if let Ok(content) = std::fs::read_to_string(&copy_path) {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(content));
                                    }
                                }))
                        )
                )
        )
        // Content area. Two paths:
        //
        // * Pure code file (single CodeBlock element): render via
        //   `uniform_list` so only the visible viewport slice is
        //   materialized as DOM. Removes the historical 300-line cap
        //   without tanking frame time on long files.
        //
        // * Markdown / mixed content: keep the flat-scroll path that
        //   can lay out heterogeneous elements (headings, lists,
        //   tables, prose). Markdown is capped at 20 MB at load
        //   time, which bounds element count.
        .child(
            match state.elements.as_slice() {
                [PreviewElement::CodeBlock { language, formatted_lines, total_lines }] => {
                    render_code_block_fullscreen(
                        language.clone(),
                        formatted_lines.clone(),
                        *total_lines,
                        scroll_handle,
                        highlight_line,
                    )
                }
                _ => render_markdown_body(state, list_state, selection_ctx, cx),
            }
        )
        // Bottom strip: either the active search bar or a persistent
        // shortcuts hint (discoverability for keys like `[` / `]` /
        // `o` / `/` / `Y`). Exactly one of the two is visible at a
        // time — the search bar takes over when `/` opens.
        .child(match search_for_this {
            Some(search) => render_preview_search_bar(&search),
            None => render_preview_hint_bar(state),
        })
        ) // end content column
        // TOC overlay paints on top of the content column when open.
        // Absolute-positioned, covers the panel, its own backdrop
        // dismisses on click-outside.
        .when_some(toc_for_this, |d, toc_state| {
            d.child(crate::preview_toc::render_toc_overlay(&toc_state, state, cx))
        })
        .into_any_element()
}

/// Persistent shortcut-hint strip at the bottom of the preview
/// panel. Replaced by the search bar while `/` search is active
/// (see the call site). Content adapts to the preview kind — the
/// hints we show only list shortcuts that *work* on the active
/// document, so users don't get taught `/` on a markdown (where
/// search is currently scoped to code files only) or `o` on a `.rs`
/// file that has no headings.
///
/// This bar is the main discoverability surface for the preview
/// shortcut set. Without it users have no way to learn `[` / `]` /
/// `o` / `/` exist — no tooltips or menu entries expose them today.
#[cfg(feature = "gpui")]
fn render_preview_hint_bar(state: &PreviewState) -> AnyElement {
    let is_code_only = matches!(
        state.elements.as_slice(),
        [PreviewElement::CodeBlock { .. }]
    );
    let has_headings = !state.headings.is_empty();
    // Build the hint string from what this doc actually supports.
    // Keeping each group to `key  action` with a middle-dot
    // separator matches the existing search-bar hint style for
    // visual consistency.
    let hint = match (is_code_only, has_headings) {
        (true, _) => "/  find    n / N  next / prev    Y  copy all    c  copy block",
        (false, true) => "[ ]  prev / next heading    o  toc    Y  copy all",
        (false, false) => "Y  copy all",
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(4.0))
        .bg(rgb(crate::theme::SURFACE_DIM))
        .border_t_1()
        .border_color(rgb(crate::theme::SURFACE_RAISED))
        .flex_shrink_0()
        .text_xs()
        .text_color(rgb(crate::theme::TEXT_DIM))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(hint)
        .into_any_element()
}

/// Bottom bar showing the `/` search query, match counter, and hint
/// keys. Mirrors mdterm's bottom-of-screen search input layout.
#[cfg(feature = "gpui")]
fn render_preview_search_bar(search: &crate::preview_search::PreviewSearchState) -> AnyElement {
    let total = search.matches.len();
    // Human-indexed counter: 1-based so the user sees "3/12" not
    // "2/12" for the third match. Zero-match case shows "0/0" so
    // the UI doesn't collapse while the user is typing partial
    // queries with no hits yet.
    let counter = if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", search.current_idx + 1, total)
    };
    let hint = if search.input_active {
        "Enter commit · Esc close"
    } else {
        "n next · N prev · Esc close"
    };
    let query_display = if search.query.is_empty() && search.input_active {
        "_".to_string()
    } else {
        // Trailing cursor marker when in input mode, so the user can
        // see where the next character will land.
        if search.input_active {
            format!("{}_", search.query)
        } else {
            search.query.clone()
        }
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py(px(4.0))
        .bg(rgb(crate::theme::SURFACE_DIM))
        .border_t_1()
        .border_color(rgb(crate::theme::SURFACE_RAISED))
        .flex_shrink_0()
        .text_xs()
        .child(
            div()
                .text_color(rgb(crate::theme::ACCENT))
                .font_weight(FontWeight::SEMIBOLD)
                .child("/"),
        )
        .child(
            div()
                .flex_1()
                .text_color(rgb(crate::theme::TEXT))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(query_display),
        )
        .child(
            div()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .child(counter),
        )
        .child(
            div()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .child(hint),
        )
        .into_any_element()
}

/// Render the markdown / mixed-element body via `gpui::list`. The
/// list element virtualizes variable-height children and exposes a
/// `scroll_to_reveal_item(idx)` method we use for TOC-driven and `[`
/// / `]` navigation. The markdown element count is bounded by the
/// 20 MB load cap, so the work of feeding `list` is cheap per frame.
///
/// If `list_state` is `None` (edge case: element count changed
/// between `sync_preview_list_states` and this render — shouldn't
/// happen in practice, but the borrow chain is lossy), we fall back
/// to the old flat `overflow_y_scroll` layout so the user still sees
/// content instead of a blank pane.
#[cfg(feature = "gpui")]
fn render_markdown_body(
    state: &PreviewState,
    list_state: Option<gpui::ListState>,
    selection_ctx: Option<crate::preview_selection::SelectionRenderCtx>,
    cx: &mut gpui::Context<GpuiShellView>,
) -> AnyElement {
    use gpui_component::ElementExt;

    // `on_prepaint` captures the body's window-space bounds each
    // frame into the view's `preview_body_bounds` cache — mouse
    // handlers read from there to convert window coords into content
    // coords. The prepaint body uses `view_entity.update(cx, ...)`
    // because `on_prepaint` hands us `&mut App`, not `&mut
    // Context<View>`.
    let view_entity = cx.entity().clone();
    let on_prepaint_body = move |bounds: gpui::Bounds<gpui::Pixels>,
                                 _w: &mut gpui::Window,
                                 cx: &mut gpui::App| {
        let _ = view_entity.update(cx, |this, _| {
            this.preview_body_bounds = Some(bounds);
        });
    };
    let on_mouse_down = cx.listener(|this, event: &gpui::MouseDownEvent, _w, cx| {
        this.preview_selection_mouse_down(event.position, cx);
    });
    let on_mouse_move = cx.listener(|this, event: &gpui::MouseMoveEvent, _w, cx| {
        this.preview_selection_mouse_move(event.position, cx);
    });
    let on_mouse_up = cx.listener(|this, _event: &gpui::MouseUpEvent, _w, cx| {
        this.preview_selection_mouse_up(cx);
    });

    let Some(list_state) = list_state else {
        // Fallback path: no list_state means the view hasn't synced
        // one yet (rare — first frame, or an edge-case race). Wire
        // selection into the `overflow_y_scroll` div directly.
        return div()
            .id(gpui::ElementId::Name("preview-content".into()))
            .flex_1()
            .overflow_y_scroll()
            .px(px(20.0))
            .py(px(16.0))
            .on_prepaint(on_prepaint_body)
            .on_mouse_down(gpui::MouseButton::Left, on_mouse_down)
            .on_mouse_move(on_mouse_move)
            .on_mouse_up(gpui::MouseButton::Left, on_mouse_up)
            .children(
                state.elements.iter().enumerate()
                    .map(|(idx, el)| render_element(el, idx, selection_ctx.as_ref())),
            )
            .into_any_element();
    };
    // The render closure runs for each visible item and must be
    // `'static`, so it captures an owned snapshot of the elements.
    // Cloning once per frame is cheap in normal use (markdown element
    // counts are in the hundreds, not millions) and keeps the data
    // model simple — no Arc on `PreviewState.elements`.
    let elements: std::sync::Arc<[PreviewElement]> = state.elements.clone().into();
    // Clone the selection ctx into the render closure. It's tiny
    // (3 f32s + an Hsla), so the per-item clone cost is negligible.
    let selection_ctx_for_items = selection_ctx.clone();
    let render_item = move |idx: usize, _w: &mut gpui::Window, _a: &mut gpui::App| -> AnyElement {
        let el = &elements[idx];
        // Padding lives on each item because `list` doesn't accept a
        // padding style that would show between items *and* at the
        // edges the way a simple `.px().py()` on a scroll container
        // does. Horizontal padding per item matches the old layout's
        // `px_5`-equivalent indent.
        div()
            .px(px(20.0))
            .child(render_element(el, idx, selection_ctx_for_items.as_ref()))
            .into_any_element()
    };
    // Wrap the list in a flex-col div so we have a place to hang the
    // mouse handlers + prepaint bounds capture. `list` is not an
    // InteractiveElement so it can't carry handlers directly. The
    // wrapper must itself be a flex container (`.flex().flex_col()`)
    // because the inner `list` uses `.flex_1()` to claim remaining
    // height — without a flex parent that flex_1 is a no-op and the
    // list renders at zero height (blank markdown body).
    div()
        .flex_1()
        .flex()
        .flex_col()
        .on_prepaint(on_prepaint_body)
        .on_mouse_down(gpui::MouseButton::Left, on_mouse_down)
        .on_mouse_move(on_mouse_move)
        .on_mouse_up(gpui::MouseButton::Left, on_mouse_up)
        .child(
            list(list_state, render_item)
                .flex_1()
                .py(px(16.0)),
        )
        .into_any_element()
}

/// Render a single top-level code block as a virtualized, full-panel
/// viewer. `uniform_list` only materializes the lines actually in the
/// viewport, so this stays O(viewport) even for 100k-line logs.
///
/// Why not reuse `render_element`: the embedded-code path (code
/// blocks inside a rendered markdown document) needs the block's
/// height to be bounded by its own content, so it flows naturally
/// between preceding and following markdown elements. The full-file
/// path needs the block to fill the remaining panel height and own
/// its own scroll. Two different layout constraints — two render
/// paths.
#[cfg(feature = "gpui")]
fn render_code_block_fullscreen(
    language: String,
    formatted_lines: Vec<(String, String, u32)>,
    total_lines: usize,
    scroll_handle: gpui::UniformListScrollHandle,
    highlight_line: Option<usize>,
) -> AnyElement {
    let lines: Arc<[(String, String, u32)]> = Arc::from(formatted_lines);
    // Gutter width in pixels: we size it once here from the first
    // row's character count and pass it to every row. Without a
    // fixed pixel width, each row independently measures its own
    // gutter text, and GPUI's per-row text shaping produces sub-pixel
    // variance in the measured width — the `1px` divider that sits
    // flex-next drifts horizontally by fractions of a pixel from row
    // to row, and the stacked dividers visually form a wavy line
    // instead of a ruler. Fixing the gutter width pins every row's
    // divider at the exact same x.
    //
    // `7.5` is an over-estimate of monospace advance at `text_xs`
    // (12 px) — Menlo/Monaco/Cascadia all sit around 7.2 px. Over-
    // estimating by 0.3 px per digit leaves a small gap to the
    // divider and absorbs any remaining measurement jitter.
    let gutter_chars = lines.first().map(|(n, _, _)| n.chars().count()).unwrap_or(2);
    let gutter_px = px(gutter_chars as f32 * 7.5 + 16.0);
    let list_lines = lines.clone();

    div()
        .flex_1()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(rgb(crate::theme::SURFACE_DIM))
        // Language + line-count header (shown only when non-empty)
        .when(!language.is_empty() || total_lines > 0, |d| {
            d.child(
                div()
                    .flex()
                    .justify_between()
                    .px_3()
                    .py(px(3.0))
                    .bg(rgb(crate::theme::SURFACE_DIM))
                    .border_b_1()
                    .border_color(rgb(crate::theme::SURFACE_RAISED))
                    .text_xs()
                    .text_color(rgb(crate::theme::TEXT_DIM))
                    .flex_shrink_0()
                    .child(if language.is_empty() { "text".to_string() } else { language.clone() })
                    .child(format!("{} lines", total_lines))
            )
        })
        .child(
            uniform_list(
                "preview-code-body",
                lines.len(),
                move |range, _window, _cx| {
                    let lines = list_lines.clone();
                    range.map(move |i| {
                        let (num, text, color) = &lines[i];
                        let is_match = Some(i) == highlight_line;
                        build_code_row(num, text, *color, gutter_px, is_match)
                    }).collect()
                },
            )
            .flex_1()
            .py(px(4.0))
            .track_scroll(&scroll_handle)
        )
        .into_any_element()
}

/// Build a single virtualized code row: `[gutter | 1px divider | text]`.
/// Stacked flush, these rows' per-row dividers line up into the
/// continuous vertical rule — this only stays true if every row's
/// gutter has the **exact same pixel width**, which is why the caller
/// computes `gutter_px` once and threads it through.
#[cfg(feature = "gpui")]
fn build_code_row(
    num: &str,
    text: &str,
    color: u32,
    gutter_px: gpui::Pixels,
    is_match: bool,
) -> AnyElement {
    // `is_match`: the row holds the currently-selected search match.
    // We tint the whole row so the eye finds it even on long lines
    // where the match text is off-screen. Per-character inline
    // highlighting is a Tranche C refinement.
    div()
        .flex()
        .text_xs()
        .when(is_match, |d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
        .child(
            div()
                .flex_shrink_0()
                .w(gutter_px)
                .pr(px(8.0))
                .text_right()
                .text_color(rgb(crate::theme::TEXT_DIM))
                .whitespace_nowrap()
                .child(num.to_string())
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(1.0))
                .bg(rgb(crate::theme::BORDER))
        )
        .child(
            div()
                .flex_1()
                .pl(px(10.0))
                .pr(px(8.0))
                .text_color(rgb(color))
                .whitespace_nowrap()
                .child(text.to_string())
        )
        .into_any_element()
}

#[cfg(feature = "gpui")]
fn render_element(
    el: &PreviewElement,
    element_idx: usize,
    selection_ctx: Option<&crate::preview_selection::SelectionRenderCtx>,
) -> AnyElement {
    use crate::preview_selection::{SelectableText, TextLocation};
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
                        .child(
                            SelectableText::new(
                                TextLocation::new(element_idx, 0),
                                text.clone(),
                            )
                            .with_selection_ctx(selection_ctx.cloned()),
                        )
                )
                .when(*level <= 2, |d| {
                    d.child(
                        div().w_full().h(px(1.0)).mt(px(4.0)).bg(rgb(crate::theme::SURFACE_RAISED))
                    )
                })
                .into_any_element()
        }

        PreviewElement::Paragraph { spans } => {
            div()
                .mb(px(10.0))
                .child(render_spans(spans, TextLocation::new(element_idx, 0), selection_ctx))
                .into_any_element()
        }

        PreviewElement::CodeBlock { language, formatted_lines, total_lines } => {
            // This path renders code blocks **inside** markdown
            // documents (fenced `````lang` blocks). The pure-code-file
            // path lives in `render_code_block_fullscreen` and uses
            // virtualization. Here we lay out every line directly
            // because embedded code blocks must size to their content
            // so they flow between surrounding markdown elements —
            // `uniform_list` needs a bounded scroll container which
            // would disrupt that flow.
            let lang_label = if language.is_empty() { None } else { Some(language.clone()) };

            div()
                .mb(px(10.0))
                .w_full()
                .bg(rgb(crate::theme::SURFACE_DIM))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(crate::theme::SURFACE_RAISED))
                .overflow_hidden()
                .when_some(lang_label, |d, lang| {
                    d.child(
                        div()
                            .flex()
                            .justify_between()
                            .px_3()
                            .py(px(3.0))
                            .bg(rgb(crate::theme::SURFACE_DIM))
                            .border_b_1()
                            .border_color(rgb(crate::theme::SURFACE_RAISED))
                            .text_xs()
                            .text_color(rgb(crate::theme::TEXT_DIM))
                            .child(lang)
                            .child(format!("{} lines", total_lines))
                    )
                })
                .child(
                    div()
                        .flex()
                        .text_xs()
                        // Gutter: line numbers, right-aligned
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_shrink_0()
                                .py(px(4.0))
                                .pl(px(8.0))
                                .pr(px(8.0))
                                .text_color(rgb(crate::theme::TEXT_DIM))
                                .children(formatted_lines.iter().map(|(num, _, _)| {
                                    div()
                                        .text_right()
                                        .whitespace_nowrap()
                                        .child(num.clone())
                                        .into_any_element()
                                }))
                        )
                        // Separator: continuous 1px vertical line
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(1.0))
                                .bg(rgb(crate::theme::BORDER))
                        )
                        // Code: syntax-highlighted content. One
                        // SelectableText per line — sub_idx encodes
                        // the line index so the extractor can slice
                        // code block ranges cleanly at newlines.
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .py(px(4.0))
                                .pl(px(10.0))
                                .pr(px(8.0))
                                .overflow_hidden()
                                .children(formatted_lines.iter().enumerate().map(|(line_idx, (_, text, color))| {
                                    div()
                                        .text_color(rgb(*color))
                                        .whitespace_nowrap()
                                        .child(
                                            SelectableText::new(
                                                TextLocation::new(element_idx, line_idx),
                                                text.clone(),
                                            )
                                            .with_selection_ctx(selection_ctx.cloned()),
                                        )
                                        .into_any_element()
                                }))
                        )
                )
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
            // Bullet text deliberately NOT selectable — it's a
            // rendering artifact, not part of the source markdown.
            // A `SelectableText` wrap on the bullet would let users
            // copy "•" tokens into their paste, which is almost
            // never what they want.
            div()
                .mb(px(3.0))
                .pl(px(indent))
                .flex()
                .gap(px(6.0))
                .child(
                    div().text_sm().text_color(rgb(crate::theme::TEXT_DIM)).w(px(16.0))
                        .flex_shrink_0()
                        .child(bullet)
                )
                .child(render_spans(spans, TextLocation::new(element_idx, 0), selection_ctx))
                .into_any_element()
        }

        PreviewElement::HorizontalRule => {
            div()
                .my_3()
                .w_full()
                .h(px(1.0))
                .bg(rgb(crate::theme::BORDER))
                .into_any_element()
        }

        PreviewElement::Blockquote { spans } => {
            div()
                .mb(px(8.0))
                .pl(px(12.0))
                .border_l_2()
                .border_color(rgb(crate::theme::TEXT_DIM))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(crate::theme::TEXT_DIM))
                        .italic()
                        .child(render_spans(spans, TextLocation::new(element_idx, 0), selection_ctx))
                )
                .into_any_element()
        }

        PreviewElement::Table { headers, rows } => {
            // Flatten (row, col) to sub_idx via `row * col_count + col`,
            // with the header row at row=0. Extraction depends on this
            // encoding — see TextLocation's doc for the invariant.
            let col_count = headers.len();
            div()
                .mb(px(12.0))
                .w_full()
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
                .overflow_hidden()
                // Header row
                .child(
                    div()
                        .flex()
                        .bg(rgb(crate::theme::SURFACE_RAISED))
                        .border_b_1()
                        .border_color(rgb(crate::theme::BORDER))
                        .children(headers.iter().enumerate().map(|(col_idx, cell_spans)| {
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(5.0))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(crate::theme::TEXT))
                                .border_r_1()
                                .border_color(rgb(crate::theme::BORDER))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(render_spans(
                                    cell_spans,
                                    TextLocation::new(element_idx, col_idx),
                                    selection_ctx,
                                ))
                                .into_any_element()
                        }))
                )
                // Data rows
                .children(rows.iter().enumerate().map(|(row_idx, row)| {
                    let row_bg = if row_idx % 2 == 0 { rgb(crate::theme::SURFACE) } else { rgb(crate::theme::SURFACE_RAISED) };
                    div()
                        .flex()
                        .bg(row_bg)
                        .when(row_idx + 1 < rows.len(), |d| {
                            d.border_b_1().border_color(rgb(crate::theme::SURFACE_RAISED))
                        })
                        .children(row.iter().enumerate().map(|(col_idx, cell_spans)| {
                            // +1 offset on the row index because sub_idx=0..col_count is reserved for the header row.
                            let sub_idx = (row_idx + 1) * col_count + col_idx;
                            div()
                                .flex_1()
                                .px_2()
                                .py(px(4.0))
                                .text_xs()
                                .text_color(rgb(crate::theme::TEXT))
                                .border_r_1()
                                .border_color(rgb(crate::theme::SURFACE_RAISED))
                                .overflow_hidden()
                                .child(render_spans(
                                    cell_spans,
                                    TextLocation::new(element_idx, sub_idx),
                                    selection_ctx,
                                ))
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
    }
}

#[cfg(feature = "gpui")]
fn render_spans(
    spans: &[TextSpan],
    location: crate::preview_selection::TextLocation,
    selection_ctx: Option<&crate::preview_selection::SelectionRenderCtx>,
) -> AnyElement {
    use crate::preview_selection::SelectableText;
    use gpui::{FontStyle, HighlightStyle, Hsla};

    // Concatenate every span into a single `SelectableText`. Per-span
    // styling becomes a HighlightStyle run over a byte range, so the
    // whole paragraph/list-item/blockquote/cell is one text layout
    // from gpui's perspective. That layout is what lets us ask
    // "what byte is at (x, y)?" in Step 4 — a per-span stack of divs
    // would give us per-span layouts that don't compose.
    //
    // Styling trade-off vs the previous per-span divs: we lose the
    // inline-code `bg + rounded + px + py` chip look because
    // HighlightStyle doesn't expose corner radius or padding. Code
    // spans still stand out via `background_color + color`, which is
    // the part users actually rely on to spot them. Full chip styling
    // would require a per-span sub-element and break selection
    // continuity, so we trade pixel-perfect chips for selectable
    // text.
    let mut text = String::new();
    let mut runs: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        let end = text.len();

        let mut style = HighlightStyle::default();
        let mut styled = false;
        if span.bold {
            style.font_weight = Some(FontWeight::BOLD);
            styled = true;
        }
        if span.italic {
            style.font_style = Some(FontStyle::Italic);
            styled = true;
        }
        if span.code {
            style.color = Some(Hsla::from(rgb(crate::theme::DANGER)));
            style.background_color = Some(Hsla::from(rgb(crate::theme::SURFACE_RAISED)));
            styled = true;
        }
        if span.link_url.is_some() {
            style.color = Some(Hsla::from(rgb(crate::theme::ACCENT)));
            // Underline to hint clickability even without a hover —
            // the previous div-based render relied on color alone,
            // but now that we're inside a single text layout, an
            // underline reads as part of the text, not as an extra
            // div, so we can afford it.
            style.underline = Some(gpui::UnderlineStyle {
                thickness: px(1.0),
                color: Some(Hsla::from(rgb(crate::theme::ACCENT))),
                wavy: false,
            });
            styled = true;
        }
        if styled && end > start {
            runs.push((start..end, style));
        }
    }

    div()
        .text_sm()
        .text_color(rgb(crate::theme::TEXT))
        .child(
            SelectableText::new(location, text)
                .with_highlights(runs)
                .with_selection_ctx(selection_ctx.cloned()),
        )
        .into_any_element()
}

// ─── File Picker ───────────────────────────────────────────────

#[cfg(feature = "gpui")]
impl FilePickerState {
    /// Empty picker shown synchronously while `scan_all_files` runs
    /// on a background thread. The caller fills it in via
    /// `apply_scan` once the walk completes. Exists so Ctrl+P opens
    /// on the same frame as the keystroke instead of stalling the
    /// render thread for the full recursive `read_dir`.
    pub fn loading(cwd: Option<String>) -> Self {
        Self {
            query: String::new(),
            all_files: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
            base_dir: cwd,
        }
    }

    /// Replace the cached file list with a freshly-scanned one and
    /// re-run the current query filter. Preserves `query` and
    /// `selected_index` if they still make sense so a user who
    /// started typing during the scan doesn't lose their input.
    pub fn apply_scan(&mut self, all_files: Vec<(String, std::time::SystemTime)>) {
        self.all_files = all_files;
        self.matches = Self::filter_files(&self.all_files, &self.query, 20);
        if self.selected_index >= self.matches.len() {
            self.selected_index = 0;
        }
    }

    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        // Filter cached file list — no filesystem access
        self.matches = Self::filter_files(&self.all_files, query, 20);
        self.selected_index = 0;
    }

    /// Filter cached file list by query. `all_files` is kept sorted
    /// by `scan_all_files` under the display rule (markdown first,
    /// then mtime desc, then path asc), so filtering here is a
    /// straight `filter().take()` — the first `max` matches are
    /// already the right ones in the right order.
    fn filter_files(
        all_files: &[(String, std::time::SystemTime)],
        query: &str,
        max: usize,
    ) -> Vec<String> {
        let query_lower = query.to_lowercase();
        if query.is_empty() {
            all_files.iter()
                .take(max)
                .map(|(p, _)| p.clone())
                .collect()
        } else {
            all_files.iter()
                .filter(|(p, _)| p.to_lowercase().contains(&query_lower))
                .take(max)
                .map(|(p, _)| p.clone())
                .collect()
        }
    }

    /// Scan filesystem once, return all previewable files paired with
    /// their last-modified time, **pre-sorted** by the same rule
    /// `filter_files` uses for display:
    ///
    /// 1. Markdown (`.md` / `.markdown`) first — highest-signal
    ///    preview target in this app.
    /// 2. Within each group, most-recently-modified first.
    /// 3. Path asc as a deterministic tie-breaker.
    ///
    /// Sorting here (not at display time) is load-bearing: the 200-
    /// item cap in `walk_dir` stops scanning the moment it fills, so
    /// if we sorted only at display time, any markdown file sitting
    /// past the 200th filesystem-order entry would be invisible.
    /// Sorting at scan time **after** the walk also doesn't help
    /// because `filter_files` used to `take(max * 3)` in filesystem
    /// order before sorting — same effective truncation. Sort once
    /// here so every downstream consumer sees the right order.
    ///
    /// Blocking — call from a background thread for Ctrl+P. The
    /// mtime is captured during the walk so the UI path never
    /// has to `stat`.
    pub fn scan_all_files(cwd: Option<String>) -> Vec<(String, std::time::SystemTime)> {
        let cwd = cwd.map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let mut results = Vec::new();
        // Walk cap is generous (5000) so the sort below sees every
        // markdown file in the tree. The old 200 cap truncated in
        // filesystem order before sorting, which meant markdown files
        // buried past inode 200 were invisible regardless of the
        // sort rule. Depth limit + ignore list still bound total
        // work; 5000 items sort in microseconds.
        Self::walk_dir(&cwd, &cwd, &mut results, 5000, 0);
        results.sort_by(|a, b| {
            let a_md = a.0.ends_with(".md") || a.0.ends_with(".markdown");
            let b_md = b.0.ends_with(".md") || b.0.ends_with(".markdown");
            b_md.cmp(&a_md)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        results
    }

    fn walk_dir(
        base: &std::path::Path,
        dir: &std::path::Path,
        results: &mut Vec<(String, std::time::SystemTime)>,
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

                // Fall back to UNIX_EPOCH so files whose mtime can't
                // be read (unusual FS, permissions) sort to the
                // bottom of their group instead of being dropped.
                let mtime = entry.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);

                results.push((rel_path, mtime));
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
    let query_color = if picker.query.is_empty() { rgb(crate::theme::TEXT_DIM) } else { rgb(crate::theme::TEXT) };

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
                .bg(rgb(crate::theme::SURFACE))
                .border_1()
                .border_color(rgb(crate::theme::BORDER))
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
                        .border_color(rgb(crate::theme::SURFACE_RAISED))
                        .child(
                            div().text_xs().text_color(rgb(crate::theme::ACCENT)).child("🔍")
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
                                        .text_color(rgb(crate::theme::TEXT_DIM))
                                        .child("No files found")
                                        .into_any_element()
                                ]
                            } else {
                                picker.matches.iter().enumerate().map(|(i, path)| {
                                    let is_selected = i == picker.selected_index;
                                    let bg = if is_selected { rgb(crate::theme::SURFACE_RAISED) } else { rgb(crate::theme::SURFACE) };
                                    let text_c = if is_selected { rgb(crate::theme::TEXT) } else { rgb(crate::theme::TEXT_DIM) };

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
                                        .hover(|d| d.bg(rgb(crate::theme::SURFACE_RAISED)))
                                        .when(is_selected, |d| d.border_l_2().border_color(rgb(crate::theme::ACCENT)))
                                        .child(icon.to_string())
                                        .child(path.clone())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            crate::preview_open::open_preview_from_picker(this, cx, i);
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
                        .border_color(rgb(crate::theme::SURFACE_RAISED))
                        .text_xs()
                        .text_color(rgb(crate::theme::TEXT_DIM))
                        .child("Enter: preview  ↑↓: navigate  Esc: close")
                )
        )
        .into_any_element()
}

#[cfg(all(test, feature = "gpui"))]
mod tests {
    use super::*;

    fn h(level: u8, text: &str) -> PreviewElement {
        PreviewElement::Heading { level, text: text.into() }
    }
    fn p(text: &str) -> PreviewElement {
        PreviewElement::Paragraph {
            spans: vec![TextSpan {
                text: text.into(),
                bold: false,
                italic: false,
                code: false,
                link_url: None,
            }],
        }
    }

    #[test]
    fn headings_collected_in_document_order() {
        let els = vec![h(1, "A"), p("a body"), h(2, "A.1"), h(1, "B")];
        let idx = build_headings_index(&els);
        assert_eq!(idx.len(), 3);
        assert_eq!(idx[0].text, "A");
        assert_eq!(idx[0].level, 1);
        assert_eq!(idx[0].element_idx, 0);
        assert_eq!(idx[1].text, "A.1");
        assert_eq!(idx[1].element_idx, 2);
        assert_eq!(idx[2].text, "B");
        assert_eq!(idx[2].element_idx, 3);
    }

    #[test]
    fn section_end_respects_heading_levels() {
        // H1 A  → section extends until next H1 (B), past H2 children.
        // H2 A.1 → section extends until next H1-or-H2 (B).
        // H1 B  → last, runs to end.
        let els = vec![
            h(1, "A"),          // 0
            p("intro"),         // 1
            h(2, "A.1"),        // 2
            p("body of A.1"),   // 3
            h(3, "A.1.1"),      // 4
            p("deeper"),        // 5
            h(1, "B"),          // 6
            p("B body"),        // 7
        ];
        let idx = build_headings_index(&els);
        assert_eq!(idx.len(), 4);
        // A spans 0..6 (up to but not including H1 B).
        assert_eq!(idx[0].element_idx, 0);
        assert_eq!(idx[0].section_end_idx, 6);
        // A.1 spans 2..6 (up to B; H3 A.1.1 is a child, doesn't terminate).
        assert_eq!(idx[1].element_idx, 2);
        assert_eq!(idx[1].section_end_idx, 6);
        // A.1.1 spans 4..6 (H1 B terminates it).
        assert_eq!(idx[2].element_idx, 4);
        assert_eq!(idx[2].section_end_idx, 6);
        // B runs to end of document.
        assert_eq!(idx[3].element_idx, 6);
        assert_eq!(idx[3].section_end_idx, 8);
    }

    #[test]
    fn content_text_concatenates_section_elements() {
        let els = vec![h(1, "Title"), p("first para"), p("second para")];
        let idx = build_headings_index(&els);
        // content_text is the joined plain text of the section —
        // used later for fuzzy match against body content, not just
        // heading titles.
        assert!(idx[0].content_text.contains("Title"));
        assert!(idx[0].content_text.contains("first para"));
        assert!(idx[0].content_text.contains("second para"));
    }

    #[test]
    fn no_headings_yields_empty_index() {
        let els = vec![p("just a paragraph")];
        assert!(build_headings_index(&els).is_empty());
    }
}
