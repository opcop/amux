use crate::commands::PaletteCommand;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandPalette {
    pub open: bool,
    pub query: String,
    pub selected_index: usize,
    pub commands: Vec<PaletteCommand>,
}

impl CommandPalette {
    pub fn render_text(&self) -> String {
        if self.open {
            let query = if self.query.is_empty() {
                "all".to_string()
            } else {
                self.query.clone()
            };
            let mut lines = vec![
                "Command Palette: open".to_string(),
                format!("  query: {query}"),
                format!("  selected: {}", self.selected_index),
            ];
            let mut current_category = String::new();
            for (index, cmd) in self.commands.iter().enumerate() {
                let cat = cmd.category.label().to_string();
                if cat != current_category {
                    lines.push(format!("  [{}]", cat));
                    current_category = cat;
                }
                let marker = if index == self.selected_index { ">" } else { "-" };
                let kb = cmd.keybinding.as_deref().unwrap_or("");
                let kb_display = if kb.is_empty() {
                    String::new()
                } else {
                    format!("  ({})", kb)
                };
                lines.push(format!("  {marker} {}{kb_display}", cmd.label));
            }
            lines.join("\n")
        } else {
            "Command Palette: closed".into()
        }
    }
}
