use crate::TabSnapshot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabStrip {
    tabs: Vec<TabSnapshot>,
}

impl TabStrip {
    pub fn new(tabs: Vec<TabSnapshot>) -> Self {
        Self { tabs }
    }

    pub fn render_text(&self) -> String {
        if self.tabs.is_empty() {
            return "[no tabs]".into();
        }

        self.tabs
            .iter()
            .map(|tab| {
                if tab.is_active {
                    format!("[{}:{}]", tab.surface_kind, tab.title)
                } else {
                    format!(" {}:{} ", tab.surface_kind, tab.title)
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}
