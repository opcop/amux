//! Modal picker UIs: template layout browser, agent launcher,
//! and new-tab-type dropdown menu. Extracted from gpui_entry.rs.

#[cfg(feature = "gpui")]
use gpui::{Context, Window};

#[cfg(feature = "gpui")]
use crate::gpui_entry::GpuiShellView;
#[cfg(feature = "gpui")]
use crate::state::{AgentPickerState, NewTabPickerItem, NewTabPickerState, TemplatePickerState};

#[cfg(feature = "gpui")]
impl GpuiShellView {
    pub(crate) fn apply_template(&mut self, template: &amux_platform::terminal::manager::LayoutTemplate) {
        let mut tm = amux_platform::terminal::manager::TerminalManager::from_template(template);
        tm.set_scrollback(self.config.scrollback);
        let ws_name = self.workspace_name();
        tm.set_workspace_name(&ws_name);
        self.workspace_terminals.insert(self.active_workspace_id.clone(), tm);
        let (shell, args) = Self::default_shell();
        let cwd = self.spawn_cwd();
        let pane_ids: Vec<_> = self.terminal_manager().active_layout()
            .map(|l| l.pane_ids()).unwrap_or_default();
        for pid in pane_ids {
            self.terminal_manager_mut().spawn_all_tabs_in_pane(&pid, &shell, &args, cwd.as_deref());
        }
        self.save_all_layouts();
    }

    /// Save current layout as a custom template.
    #[allow(dead_code)]
    pub(crate) fn save_current_as_template(&mut self, name: &str) {
        let desc = format!("{} panes", self.terminal_manager().total_panes());
        let template = self.terminal_manager().to_template(name, &desc);
        Self::save_template(&template);
    }

    pub(crate) fn open_template_picker(&mut self) {
        let templates = Self::all_templates();
        if templates.is_empty() { return; }
        self.template_picker = Some(TemplatePickerState {
            templates,
            selected_index: 0,
        });
    }

    pub(crate) fn execute_template_picker(&mut self) {
        if let Some(picker) = self.template_picker.take() {
            if let Some(t) = picker.templates.get(picker.selected_index) {
                self.apply_template(t);
            }
        }
    }

    pub(crate) fn delete_selected_template(&mut self) {
        if let Some(ref mut picker) = self.template_picker {
            if let Some(t) = picker.templates.get(picker.selected_index) {
                if t.builtin { return; }
                let name = t.name.clone();
                Self::delete_template(&name);
                picker.templates.remove(picker.selected_index);
                if picker.templates.is_empty() {
                    self.template_picker = None;
                } else if picker.selected_index >= picker.templates.len() {
                    picker.selected_index = picker.templates.len() - 1;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn open_agent_picker(&mut self) {
        let mut agents: Vec<(String, String, bool)> = Vec::new();
        if self.wsl_supported() && self.wsl_detected {
            agents.push(("wsl".into(), "WSL Terminal".into(), true));
        }
        for &(tool_id, label, _env) in &self.detected_vibe_tools {
            agents.push((tool_id.into(), label.into(), false));
        }
        if agents.is_empty() { return; }
        if agents.len() == 1 {
            let (tool_id, _, is_wsl) = &agents[0];
            if *is_wsl {
                self.launch_wsl_shell();
            } else {
                self.launch_vibe_tool_env(tool_id, false);
            }
            return;
        }
        self.agent_picker = Some(AgentPickerState {
            agents,
            selected_index: 0,
        });
    }

    pub(crate) fn execute_agent_picker(&mut self) {
        if let Some(picker) = self.agent_picker.take() {
            if let Some((tool_id, _, is_wsl)) = picker.agents.get(picker.selected_index) {
                if *is_wsl {
                    self.launch_wsl_shell();
                } else {
                    self.launch_vibe_tool_env(tool_id, false);
                }
            }
        }
    }

    pub(crate) fn open_new_tab_picker(
        &mut self,
        pane_id: amux_platform::terminal::manager::PaneId,
        anchor: gpui::Point<gpui::Pixels>,
    ) {
        let mut items = vec![
            NewTabPickerItem { id: "terminal".into(), label: "Terminal".into(), icon: ">_", separator_after: false },
        ];
        if self.wsl_supported() && self.wsl_detected {
            items.push(NewTabPickerItem {
                id: "wsl".into(), label: "WSL Terminal".into(), icon: "🐧", separator_after: false,
            });
        }
        if !self.detected_vibe_tools.is_empty() {
            items.last_mut().unwrap().separator_after = true;
        }
        for &(tool_id, label, _env) in &self.detected_vibe_tools {
            items.push(NewTabPickerItem {
                id: tool_id.into(), label: label.into(), icon: "●", separator_after: false,
            });
        }
        items.last_mut().unwrap().separator_after = true;
        items.push(NewTabPickerItem {
            id: "preview".into(), label: "Preview File...".into(), icon: "◈", separator_after: false,
        });
        if self.browser_supported() {
            items.push(NewTabPickerItem {
                id: "browser".into(), label: "Browser".into(), icon: "◉", separator_after: false,
            });
        }
        self.new_tab_picker = Some(NewTabPickerState {
            pane_id,
            items,
            selected_index: 0,
            anchor,
        });
    }

    pub(crate) fn execute_new_tab_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.new_tab_picker.take() else { return };
        let Some(item) = picker.items.get(picker.selected_index) else { return };
        self.terminal_manager_mut().set_active_pane(&picker.pane_id);
        match item.id.as_str() {
            "terminal" => {
                let env = self.capture_active_env();
                self.terminal_manager_mut().add_tab_to_active_pane("Terminal".into());
                self.spawn_with_captured_env(&env);
            }
            "wsl" => self.launch_wsl_shell(),
            "preview" => crate::preview_open::open_file_picker(self, cx),
            "browser" => self.open_browser("", window, cx),
            tool_id => self.launch_vibe_tool_env(tool_id, false),
        }
    }
}
