#[cfg(feature = "gpui")]
use amux_ui::GpuiActiveSurfaceItem;
#[cfg(feature = "gpui")]
use gpui::{rgb, AnyElement, FontWeight, IntoElement, div, prelude::*};

#[cfg(feature = "gpui")]
pub fn render_active_surface_panel(item: Option<&GpuiActiveSurfaceItem>) -> impl IntoElement {
    let mut panel = base_panel("Active Surface", rgb(0xffffff));
    let Some(item) = item else {
        return panel.child(
            div()
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child("No active surface"),
        );
    };

    panel = panel
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x1f2933))
                .child(item.tab_title.clone()),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x4b5563))
                .child(format!("Kind: {}", item.surface_kind)),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x4b5563))
                .child(format!("Pane: {}", item.pane_id)),
        );

    for line in item.summary_lines.iter().take(6) {
        panel = panel.child(
            div()
                .text_sm()
                .text_color(rgb(0x6b7280))
                .child(line.clone()),
        );
    }

    panel.child(render_surface_view(item))
}

#[cfg(feature = "gpui")]
fn render_surface_view(item: &GpuiActiveSurfaceItem) -> AnyElement {
    match item.surface_kind {
        "editor" => render_editor_surface_view(item).into_any_element(),
        "preview" => render_preview_surface_view(item).into_any_element(),
        "agent" => render_agent_surface_view(item).into_any_element(),
        "terminal" => render_terminal_surface_view(item).into_any_element(),
        "file_tree" => render_file_tree_surface_view(item).into_any_element(),
        "settings" => render_settings_surface_view(item).into_any_element(),
        _ => render_generic_surface_view(item).into_any_element(),
    }
}

#[cfg(feature = "gpui")]
fn render_editor_surface_view(item: &GpuiActiveSurfaceItem) -> impl IntoElement {
    let path = summary_value(item, "Path:").unwrap_or_else(|| item.tab_title.clone());
    let language = summary_value(item, "Language:").unwrap_or_else(|| "text".into());
    let is_dirty = summary_value(item, "Dirty:").map(|v| v == "true").unwrap_or(false);
    let is_readonly = summary_value(item, "Readonly:").map(|v| v == "true").unwrap_or(false);
    let line_count = item.content_lines.len();

    // Dark theme language badge colors
    let (lang_bg, lang_color) = match language.to_lowercase().as_str() {
        "rust" => (rgb(0x3b1c1c), rgb(0xf87171)),
        "javascript" | "typescript" => (rgb(0x3b3510), rgb(0xfbbf24)),
        "python" => (rgb(0x1a3318), rgb(0x4ade80)),
        "markdown" => (rgb(0x2e1a47), rgb(0xc084fc)),
        "json" => (rgb(0x3b2510), rgb(0xfb923c)),
        "html" | "css" => (rgb(0x3b2010), rgb(0xfb923c)),
        _ => (rgb(0x1e293b), rgb(0x94a3b8)),
    };

    let mut content = surface_content_block(rgb(0x1e1e2e), rgb(0x313244))
        // Header bar
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .pb_2()
                .border_b_1()
                .border_color(rgb(0x313244))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xcdd6f4))
                                .child(path.clone()),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(lang_bg)
                                .text_color(lang_color)
                                .text_xs()
                                .child(language.clone()),
                        )
                        .when(is_dirty, |this| this.child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(0x3b1c1c))
                                .text_color(rgb(0xf87171))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Modified"),
                        ))
                        .when(is_readonly, |this| this.child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(0x3b3510))
                                .text_color(rgb(0xfbbf24))
                                .text_xs()
                                .child("Read-only"),
                        )),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6c7086))
                        .child(format!("{} lines", line_count)),
                ),
        );

    // Code lines with gutter
    for (index, line) in item.content_lines.iter().take(40).enumerate() {
        content = content.child(editor_content_line(index + 1, line, &language));
    }

    surface_section("Editor", content)
}

#[cfg(feature = "gpui")]
fn render_preview_surface_view(item: &GpuiActiveSurfaceItem) -> impl IntoElement {
    let preview_kind = summary_value(item, "Kind:").unwrap_or_else(|| "PlainText".into());
    let source = summary_value(item, "Source:").unwrap_or_else(|| item.tab_title.clone());
    let is_markdown = preview_kind.contains("Markdown");
    let line_count = item.content_lines.len();

    let kind_bg = if is_markdown { rgb(0x2e1a47) } else { rgb(0x1e293b) };
    let kind_color = if is_markdown { rgb(0xc084fc) } else { rgb(0x94a3b8) };

    let mut content = surface_content_block(rgb(0x1e1e2e), rgb(0x313244))
        // Header
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .pb_2()
                .border_b_1()
                .border_color(rgb(0x313244))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xcdd6f4))
                                .child(source.clone()),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(kind_bg)
                                .text_color(kind_color)
                                .text_xs()
                                .child(if is_markdown { "Markdown" } else { "Text" }.to_string()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x6c7086))
                                .child(format!("{} lines", line_count)),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(0x1a3318))
                                .text_color(rgb(0x4ade80))
                                .text_xs()
                                .child("Preview"),
                        ),
                ),
        );

    // Content
    for line in item.content_lines.iter().take(40) {
        content = content.child(if is_markdown {
            enhanced_markdown_line(line)
        } else {
            div()
                .text_sm()
                .text_color(rgb(0xbac2de))
                .child(line.to_string())
                .into_any_element()
        });
    }

    surface_section("Preview", content)
}

/// Parse agent list entry from content line
/// Format: "id|name|status|supported"
#[cfg(feature = "gpui")]
fn parse_agent_entry(line: &str) -> Option<(String, String, String, bool)> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() >= 4 {
        let supported = parts[3] == "true" || parts[3] == "supported";
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
            supported,
        ))
    } else {
        None
    }
}

#[cfg(feature = "gpui")]
fn render_agent_surface_view(item: &GpuiActiveSurfaceItem) -> impl IntoElement {
    let active_workspace = summary_value(item, "Workspace:");

    // Agent list from content_lines
    let agents: Vec<_> = item
        .content_lines
        .iter()
        .filter_map(|line| parse_agent_entry(line))
        .collect();

    let installed_count = agents.iter().filter(|(_, _, status, _)| status == "installed").count();
    let available_count = agents.iter().filter(|(_, _, _, supported)| *supported).count();

    let mut content = surface_content_block(rgb(0xfaf5ff), rgb(0xe9d5fc))
        // Header with title and counts
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .pb_2()
                .border_b_1()
                .border_color(rgb(0xe9d5fc))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_lg()
                                .text_color(rgb(0x7c3aed))
                                .child("🤖"),
                        )
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0x1e1b4b))
                                .child("AI Agents"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(rgb(0x22c55e))
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("{} installed", installed_count)),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(rgb(0x8b5cf6))
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("{} available", available_count)),
                        ),
                ),
        )
        // Workspace indicator
        .child(
            div()
                .mt_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0xf3e8ff))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b21a8))
                        .child("📁"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b21a8))
                        .child(
                            active_workspace
                                .unwrap_or_else(|| "No workspace selected".to_string()),
                        ),
                ),
        )
        // Agent list
        .child(
            div()
                .mt_3()
                .flex()
                .flex_col()
                .gap_2(),
        );

    // Render agent cards
    for (id, name, status, supported) in &agents {
        content = content.child(render_agent_card(id.clone(), name.clone(), status.clone(), *supported));
    }

    // Empty state
    if agents.is_empty() {
        content = content.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .py_8()
                .gap_3()
                .child(
                    div()
                        .text_3xl()
                        .text_color(rgb(0xd8b4fe))
                        .child("🔍"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x7c3aed))
                        .child("Detecting AI coding tools..."),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xa855f7))
                        .child("Codex, Claude Code, OpenCode, Aider"),
                ),
        );
    }

    surface_section("Agents", content)
}

#[cfg(feature = "gpui")]
fn render_agent_card(id: String, name: String, status: String, supported: bool) -> impl IntoElement {
    // Status styling
    let (status_bg, status_color, status_icon, status_text) = match status.as_str() {
        "installed" => (rgb(0xecfdf5), rgb(0x047857), "✓", "Installed"),
        "not_found" => (rgb(0xfef2f2), rgb(0xb91c1c), "✗", "Not Found"),
        "needs_auth" => (rgb(0xffedd5), rgb(0xc2410c), "🔑", "Needs Auth"),
        broken if broken.starts_with("broken:") => {
            (rgb(0xfef2f2), rgb(0xb91c1c), "⚠", "Broken")
        }
        _ => (rgb(0xf3f4f6), rgb(0x6b7280), "?", "Unknown"),
    };

    // Agent icon based on id
    let agent_icon = match id.as_str() {
        "claude" => "🦙",
        "codex" => "💻",
        "opencode" => "🚀",
        "aider" => "🤝",
        _ => "🤖",
    };

    // Get action button based on status
    let action_button = if status == "installed" && supported {
        // Green "Launch" button
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(rgb(0x22c55e))
            .text_sm()
            .text_color(rgb(0xffffff))
            .font_weight(FontWeight::MEDIUM)
            .child("▶ Launch")
    } else if !supported && status == "installed" {
        // Gray "Unsupported" button
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(rgb(0xe5e7eb))
            .text_sm()
            .text_color(rgb(0x6b7280))
            .child("WSL only")
    } else {
        // Gray "Not Available" button
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(rgb(0xe5e7eb))
            .text_sm()
            .text_color(rgb(0x9ca3af))
            .child("Not Available")
    };

    div()
        .flex()
        .items_center()
        .gap_3()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0xe5e7eb))
        .bg(rgb(0xffffff))
        .hover(|h| {
            h.border_color(rgb(0xd1d5db))
                .bg(rgb(0xfafafa))
        })
        // Agent icon
        .child(
            div()
                .w_10()
                .h_10()
                .rounded_full()
                .bg(rgb(0xf3f4f6))
                .flex()
                .items_center()
                .justify_center()
                .text_xl()
                .child(agent_icon),
        )
        // Agent info
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0x1f2937))
                                .child(name),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_full()
                                .bg(status_bg)
                                .text_xs()
                                .text_color(status_color)
                                .child(format!("{} {}", status_icon, status_text)),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child(id),
                ),
        )
        // Action button
        .child(action_button)
}

#[cfg(feature = "gpui")]
fn render_terminal_surface_view(item: &GpuiActiveSurfaceItem) -> impl IntoElement {
    let provider = summary_value(item, "Provider:");
    let mode = summary_value(item, "Mode:");
    let cwd = summary_value(item, "CWD:").unwrap_or_else(|| "unset".into());
    let session_id = summary_value(item, "Session:");

    let terminal_title = provider.clone().unwrap_or_else(|| item.tab_title.clone());

    // Determine terminal color scheme based on session type
    let (term_bg, term_border, header_bg, prompt_color, output_color) =
        if terminal_title.contains("codex") || terminal_title.contains("claude") {
            // AI agent terminals - purple theme
            (rgb(0x0f0a1a), rgb(0x4c1d95), rgb(0x1e1b4b), rgb(0xc4b5fd), rgb(0xe9d5ff))
        } else {
            // Regular terminals - Catppuccin theme
            (rgb(0x1e1e2e), rgb(0x313244), rgb(0x181825), rgb(0x89b4fa), rgb(0xcdd6f4))
        };

    let mut content = surface_content_block(rgb(0x181825), term_border)
        // Header with terminal controls visual
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .pb_2()
                .border_b_1()
                .border_color(term_border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        // Terminal "window controls"
                        .child(
                            div()
                                .flex()
                                .gap_1p5()
                                .child(
                                    div()
                                        .w_3()
                                        .h_3()
                                        .rounded_full()
                                        .bg(rgb(0xef4444)),
                                )
                                .child(
                                    div()
                                        .w_3()
                                        .h_3()
                                        .rounded_full()
                                        .bg(rgb(0xfbbf24)),
                                )
                                .child(
                                    div()
                                        .w_3()
                                        .h_3()
                                        .rounded_full()
                                        .bg(rgb(0x22c55e)),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0xf9fafb))
                                .child(terminal_title.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(header_bg)
                                .text_color(rgb(0x93c5fd))
                                .text_xs()
                                .child(mode.unwrap_or_else(|| "Terminal".into())),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(0x1f2937))
                                .text_color(rgb(0x9ca3af))
                                .text_xs()
                                .child(format!("~{}", cwd.split('/').last().unwrap_or(&cwd))),
                        ),
                ),
        )
        // Session info
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .mb_2()
                .child(
                    div()
                        .w_2()
                        .h_2()
                        .rounded_full()
                        .bg(rgb(0x22c55e)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .child(format!(
                            "Session: {}",
                            session_id.unwrap_or_else(|| "unknown".into())
                        )),
                ),
        )
        // Terminal output area
        .child(
            div()
                .mt_1()
                .rounded_md()
                .bg(term_bg)
                .border_1()
                .border_color(term_border)
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .max_h_48()
                .overflow_hidden()
                .font_family("Cascadia Code, Consolas, monospace".to_string()),
        );

    for line in item.content_lines.iter().take(14) {
        content = content.child(terminal_transcript_line(line, prompt_color, output_color));
    }

    surface_section("Terminal", content)
}

/// Parse file tree content lines into structured entries
/// Format: "📁 folder_name/" for directories, "📄 file_name" for files
/// Entries may be prefixed with spaces for indentation based on depth
#[cfg(feature = "gpui")]
fn parse_file_tree_entry(line: &str) -> Option<(bool, bool, String, usize)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let depth = (line.len() - trimmed.len()) / 2; // 2 spaces per depth level
    let (is_dir, name) = if trimmed.starts_with("📁 ") {
        (true, trimmed.trim_start_matches("📁 ").trim_end_matches('/'))
    } else if trimmed.starts_with("📄 ") {
        (false, trimmed.trim_start_matches("📄 "))
    } else if trimmed.ends_with('/') {
        (true, trimmed.trim_end_matches('/'))
    } else {
        (false, trimmed)
    };

    Some((is_dir, name.is_empty(), name.to_string(), depth))
}

#[cfg(feature = "gpui")]
fn render_file_tree_surface_view(item: &GpuiActiveSurfaceItem) -> impl IntoElement {
    let filter = summary_value(item, "Filter:").unwrap_or_default();
    let selected = summary_value(item, "Selected:");
    let show_hidden = summary_value(item, "Show hidden:")
        .map(|v| v == "true")
        .unwrap_or(false);

    let filter_active = !filter.is_empty();

    let mut content = surface_content_block(rgb(0xfafbfc), rgb(0xe2e8eb))
        // Header with filter input visual and controls
        .child(
            div()
                .flex()
                .justify_between()
                .items_center()
                .pb_2()
                .border_b_1()
                .border_color(rgb(0xe2e8eb))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0x64748b))
                                .child("🔍"),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(0xf1f5f9))
                                .text_sm()
                                .text_color(if filter_active {
                                    rgb(0x1e293b)
                                } else {
                                    rgb(0x94a3b8)
                                })
                                .when(!filter_active, |this| this.child("Filter files...")),
                        )
                        .when(filter_active, |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x1e293b))
                                    .child(filter.clone()),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(if show_hidden { rgb(0x3b82f6) } else { rgb(0xf1f5f9) })
                                .text_color(if show_hidden { rgb(0xffffff) } else { rgb(0x64748b) })
                                .text_xs()
                                .child(".hidden"),
                        ),
                ),
        )
        // File tree content area
        .child(
            div()
                .mt_2()
                .flex()
                .flex_col()
                .gap_0()
                .max_h_64()
                .overflow_hidden()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xe2e8eb))
                .bg(rgb(0xffffff)),
        );

    // Render file tree entries from content_lines
    // Format in content_lines: "📁 folder/" or "📄 file.rs" with indentation
    for line in &item.content_lines {
        if let Some((is_dir, is_empty, name, depth)) = parse_file_tree_entry(line) {
            let is_selected = selected.as_ref().map(|s| s == &name).unwrap_or(false);
            content = content.child(render_file_tree_item(
                is_dir,
                is_empty,
                &name,
                depth,
                is_selected,
                &filter,
            ));
        }
    }

    // Show empty state if no content
    if item.content_lines.is_empty() {
        content = content.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .py_8()
                .gap_2()
                .child(
                    div()
                        .text_2xl()
                        .text_color(rgb(0xd1d5db))
                        .child(if filter_active { "🔍" } else { "📂" }),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x9ca3af))
                        .child(if filter_active {
                            "No matches found"
                        } else {
                            "Empty directory"
                        }),
                ),
        );
    }

    surface_section("Explorer", content)
}

#[cfg(feature = "gpui")]
fn render_file_tree_item(
    is_dir: bool,
    _is_empty: bool,
    name: &str,
    depth: usize,
    is_selected: bool,
    filter: &str,
) -> impl IntoElement {
    // Get file/directory icon and color
    let icon = if is_dir {
        "📁".to_string() // Yellow folder
    } else {
        let ext = get_file_extension(name);
        match ext.as_str() {
            "rs" => "🦀".to_string(),
            "js" | "ts" | "jsx" | "tsx" => "📜".to_string(),
            "py" => "🐍".to_string(),
            "md" => "📝".to_string(),
            "json" => "📋".to_string(),
            "toml" | "yaml" | "yml" => "⚙️".to_string(),
            "css" | "scss" => "🎨".to_string(),
            "html" => "🌐".to_string(),
            "git" => "🔀".to_string(),
            _ => "📄".to_string(),
        }
    };

    // Highlight matching text in filter
    let display_name = if !filter.is_empty() && name.to_lowercase().contains(&filter.to_lowercase()) {
        highlight_filter_match(name, filter)
    } else {
        name.to_string()
    };

    let indent = gpui::px((depth * 16) as f32); // 16px per depth level

    div()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py_1()
        .pl(indent)
        .when(is_selected, |this| {
            this.bg(rgb(0x3b82f6))
                .text_color(rgb(0xffffff))
               })
        .when(!is_selected, |this| {
            this.hover(|h| h.bg(rgb(0xf1f5f9)))
                .text_color(rgb(0x374151))
        })
        .child(
            div()
                .text_sm()
                .child(if is_dir { format!("{}/", display_name) } else { display_name }),
        )
        .child(
            div()
                .flex_1()
                .text_xs()
                .text_color(if is_selected { rgb(0xc7d2fe) } else { rgb(0x9ca3af) })
                .child(icon),
        )
}

#[cfg(feature = "gpui")]
fn get_file_extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

#[cfg(feature = "gpui")]
fn highlight_filter_match(name: &str, _filter: &str) -> String {
    // For now, just return the name with filter matching
    // A full implementation would use styled text
    name.to_string()
}

#[cfg(feature = "gpui")]
fn render_generic_surface_view(item: &GpuiActiveSurfaceItem) -> impl IntoElement {
    let mut content = surface_content_block(rgb(0xf8fafc), rgb(0xe5e7eb));

    if item.content_lines.is_empty() {
        content = content.child(
            div()
                .text_sm()
                .text_color(rgb(0x64748b))
                .child("No content preview"),
        );
    } else {
        for line in item.content_lines.iter().take(12) {
            content = content.child(content_line(line, rgb(0x334155)));
        }
    }

    surface_section("Content", content)
}

#[cfg(feature = "gpui")]
fn surface_section(title: &str, content: gpui::Div) -> gpui::Div {
    div()
        .mt_2()
        .pt_2()
        .border_t_1()
        .border_color(rgb(0x313244))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x89b4fa))
                .child(title.to_string()),
        )
        .child(content)
}

#[cfg(feature = "gpui")]
fn surface_content_block(background: gpui::Rgba, border: gpui::Rgba) -> gpui::Div {
    div()
        .mt_2()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(background)
        .p_3()
        .flex()
        .flex_col()
        .gap_1()
}

#[cfg(feature = "gpui")]
fn content_line(line: &str, color: gpui::Rgba) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(color)
        .whitespace_nowrap()
        .child(if line.is_empty() {
            " ".to_string()
        } else {
            line.to_string()
        })
}

#[cfg(feature = "gpui")]
fn editor_content_line(line_no: usize, line: &str, _language: &str) -> impl IntoElement {
    let trimmed = line.trim();
    let is_comment = trimmed.starts_with("//") || trimmed.starts_with("#");
    let is_string = line.contains('"') || line.contains('\'');
    let is_keyword = ["fn ", "let ", "const ", "mut ", "pub ", "use ", "mod ", "struct ", "enum ", "impl ", "trait ", "for ", "while ", "if ", "else ", "match ", "return "]
        .iter()
        .any(|kw| trimmed.starts_with(kw) || trimmed.contains(&format!(" {kw}")));

    let text_color = if is_comment {
        rgb(0x6c7086) // Muted gray for comments
    } else if is_string {
        rgb(0xa6e3a1) // Green for strings
    } else if is_keyword {
        rgb(0xcba6f7) // Purple for keywords
    } else {
        rgb(0xcdd6f4) // Default light text
    };

    div()
        .flex()
        .items_center()
        .hover(|this| this.bg(rgb(0x313244)))
        .font_family("Cascadia Code, Consolas, DejaVu Sans Mono, monospace".to_string())
        .child(
            div()
                .w_12()
                .px_2()
                .py_0p5()
                .text_xs()
                .text_color(rgb(0x45475a))
                .text_right()
                .border_r_1()
                .border_color(rgb(0x313244))
                .bg(rgb(0x181825))
                .child(line_no.to_string()),
        )
        .child(
            div()
                .flex_1()
                .px_3()
                .py_0p5()
                .text_sm()
                .text_color(text_color)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(if line.is_empty() {
                    " ".to_string()
                } else {
                    line.to_string()
                }),
        )
}

#[cfg(feature = "gpui")]
fn enhanced_markdown_line(line: &str) -> AnyElement {
    let trimmed = line.trim();

    // Headers (h1-h6) — golden/amber on dark
    if let Some(text) = trimmed.strip_prefix("###### ") {
        return div()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0x89b4fa))
            .child(text.to_string())
            .into_any_element();
    }
    if let Some(text) = trimmed.strip_prefix("##### ") {
        return div()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0x89b4fa))
            .child(text.to_string())
            .into_any_element();
    }
    if let Some(text) = trimmed.strip_prefix("#### ") {
        return div()
            .text_base()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0x89b4fa))
            .child(text.to_string())
            .into_any_element();
    }
    if let Some(text) = trimmed.strip_prefix("### ") {
        return div()
            .text_lg()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0x89dceb))
            .child(text.to_string())
            .into_any_element();
    }
    if let Some(text) = trimmed.strip_prefix("## ") {
        return div()
            .text_xl()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0x94e2d5))
            .mt_1()
            .child(text.to_string())
            .into_any_element();
    }
    if let Some(text) = trimmed.strip_prefix("# ") {
        return div()
            .text_2xl()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0xf9e2af))
            .mt_2()
            .mb_1()
            .child(text.to_string())
            .into_any_element();
    }

    // Code blocks
    if trimmed.starts_with("```") {
        let lang = trimmed.trim_start_matches('`').trim();
        return div()
            .my_1()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(rgb(0x11111b))
            .text_sm()
            .text_color(rgb(0x6c7086))
            .font_family("Cascadia Code, Consolas, monospace".to_string())
            .child(if lang.is_empty() { "---".to_string() } else { format!("--- {} ---", lang) })
            .into_any_element();
    }

    // Bulleted lists
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return div()
            .flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .text_color(rgb(0xf9e2af))
                    .child("•"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xbac2de))
                    .child(trimmed.trim_start_matches(|c| c == '-' || c == '*').trim().to_string()),
            )
            .into_any_element();
    }

    // Numbered lists
    if trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        && trimmed.contains(". ") {
        let parts: Vec<&str> = trimmed.splitn(2, ". ").collect();
        if parts.len() == 2 {
            return div()
                .flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(0xf9e2af))
                        .child(format!("{}.", parts[0])),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xbac2de))
                        .child(parts[1].trim().to_string()),
                )
                .into_any_element();
        }
    }

    // Blockquotes
    if trimmed.starts_with("> ") {
        return div()
            .pl_3()
            .border_l_2()
            .border_color(rgb(0x585b70))
            .my_1()
            .child(
                div()
                    .text_sm()
                    .italic()
                    .text_color(rgb(0x7f849c))
                    .child(trimmed.trim_start_matches("> ").trim().to_string()),
            )
            .into_any_element();
    }

    // Horizontal rule
    if trimmed == "---" || trimmed == "***" || trimmed == "___" {
        return div()
            .my_2()
            .h_px()
            .w_full()
            .bg(rgb(0x313244))
            .into_any_element();
    }

    // Empty line
    if line.trim().is_empty() {
        return div().h_3().into_any_element();
    }

    // Bold text
    if trimmed.contains("**") {
        return div()
            .text_sm()
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(trimmed.replace("**", "")),
            )
            .into_any_element();
    }

    // Italic text
    if trimmed.contains('*') {
        return div()
            .text_sm()
            .italic()
            .text_color(rgb(0xa6adc8))
            .child(trimmed.replace('*', ""))
            .into_any_element();
    }

    // Regular text
    div()
        .text_sm()
        .text_color(rgb(0xbac2de))
        .child(line.to_string())
        .into_any_element()
}

#[cfg(feature = "gpui")]
fn summary_value(item: &GpuiActiveSurfaceItem, prefix: &str) -> Option<String> {
    item.summary_lines
        .iter()
        .find_map(|line| line.strip_prefix(prefix).map(|value| value.trim().to_string()))
}

#[cfg(feature = "gpui")]
fn terminal_transcript_line(
    line: &str,
    prompt_color: gpui::Rgba,
    output_color: gpui::Rgba,
) -> AnyElement {
    // Session/Status lines
    if line.starts_with("Session:") || line.starts_with("Status:") {
        return div()
            .text_xs()
            .text_color(rgb(0x6b7280))
            .child(line.to_string())
            .into_any_element();
    }

    if line.starts_with("Recent IO:") {
        return div()
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0x60a5fa))
            .child("━━ Recent Output ━━".to_string())
            .into_any_element();
    }

    // Command input line (has content that's not a status)
    let is_input = !line.is_empty()
        && !line.starts_with("Session:")
        && !line.starts_with("Status:")
        && !line.starts_with("Recent")
        && !line.starts_with("━━");

    if is_input {
        return div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_base().text_color(prompt_color).child("❯"))
            .child(
                div()
                    .text_sm()
                    .text_color(output_color)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(line.to_string()),
            )
            .into_any_element();
    }

    // Output/echo lines
    div()
        .text_sm()
        .text_color(rgb(0x6b7280))
        .child(line.to_string())
        .into_any_element()
}

#[cfg(feature = "gpui")]
fn render_settings_surface_view(item: &GpuiActiveSurfaceItem) -> impl IntoElement {
    let category = summary_value(item, "Category:")
        .unwrap_or_else(|| "General".to_string());
    let count_str = summary_value(item, " categories")
        .unwrap_or_else(|| "7 categories".to_string());
    
    let categories = vec![
        ("General", "Basic application settings"),
        ("Appearance", "Theme, colors, and fonts"),
        ("Editor", "Editor behavior and formatting"),
        ("Terminal", "Terminal emulator settings"),
        ("Keyboard", "Keyboard shortcuts"),
        ("Auto-save", "Auto-save configuration"),
        ("Workspace", "Workspace preferences"),
    ];
    
    let mut content = div()
        .flex()
        .flex_col()
        .gap_4()
        .p_4();
    
    // Header
    content = content.child(
        div()
            .text_xl()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0x1f2933))
            .child("Settings")
    );
    
    // Categories list
    content = content.child(
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0x6b7280))
                    .child("CATEGORIES")
            )
    );
    
    for (cat_name, cat_desc) in categories {
        let is_selected = cat_name.to_lowercase() == category.to_lowercase();
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap_0()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(if is_selected { rgb(0xf3f4f6) } else { rgb(0xffffff) })
                .border_l_2()
                .border_color(if is_selected { rgb(0x3b82f6) } else { rgb(0xd6d3d1) })
                .child(
                    div()
                        .text_sm()
                        .font_weight(if is_selected { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                        .text_color(if is_selected { rgb(0x1f2933) } else { rgb(0x4b5563) })
                        .child(cat_name)
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x9ca3af))
                        .child(cat_desc)
                )
        );
    }
    
    // Settings panel background
    surface_content_block(rgb(0xfafafa), rgb(0xe5e7eb))
        .child(content)
}

#[cfg(feature = "gpui")]
fn base_panel(title: &str, background: gpui::Rgba) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(0xd6d3d1))
        .bg(background)
        .p_3()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_sm()
                .child(title.to_string()),
        )
}
