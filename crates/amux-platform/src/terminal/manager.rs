//! Terminal Manager — per-pane tabs + nested splits (limux-style)
//!
//! Each pane has its own tab strip. Panes can be split arbitrarily.

use std::collections::HashMap;

use crate::terminal::view::TerminalView;

/// Unique ID for a pane
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(pub String);

/// Split direction
#[derive(Clone, Debug, Copy)]
pub enum SplitDirection {
    Horizontal, // Side by side
    Vertical,   // Top and bottom
}

/// A terminal tab inside a pane
pub struct PaneTab {
    pub title: String,
    pub terminal: TerminalView,
}

/// A pane with its own tab strip (like limux)
pub struct TerminalPane {
    pub id: PaneId,
    pub tabs: Vec<PaneTab>,
    pub active_tab: usize,
}

impl TerminalPane {
    pub fn new(id: PaneId) -> Self {
        Self {
            id,
            tabs: vec![PaneTab {
                title: "Terminal".to_string(),
                terminal: TerminalView::new(),
            }],
            active_tab: 0,
        }
    }

    /// Get the active terminal
    pub fn active_terminal(&mut self) -> &mut TerminalView {
        &mut self.tabs[self.active_tab].terminal
    }

    /// Get the active terminal (immutable)
    pub fn active_terminal_ref(&self) -> &TerminalView {
        &self.tabs[self.active_tab].terminal
    }

    /// Add a new tab to this pane and make it active
    pub fn add_tab(&mut self, title: String) -> usize {
        self.tabs.push(PaneTab {
            title,
            terminal: TerminalView::new(),
        });
        self.active_tab = self.tabs.len() - 1;
        self.active_tab
    }

    /// Close a tab by index. Returns false if it's the last tab.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    /// Poll all tabs for PTY output
    pub fn poll_all_tabs(&mut self) -> bool {
        let mut any = false;
        for tab in &mut self.tabs {
            if tab.terminal.poll() {
                any = true;
            }
        }
        any
    }

    /// Tab count
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Tab titles for rendering
    pub fn tab_titles(&self) -> Vec<(usize, String, bool)> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                // Use OSC title from terminal if available, fallback to static title
                let title = t.terminal.emulator().title()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| t.title.clone());
                (i, title, i == self.active_tab)
            })
            .collect()
    }
}

/// Pane layout tree — splits of panes
#[derive(Clone, Debug)]
pub enum PaneLayout {
    Single(PaneId),
    Horizontal {
        left: Box<PaneLayout>,
        right: Box<PaneLayout>,
        ratio: f32,
    },
    Vertical {
        top: Box<PaneLayout>,
        bottom: Box<PaneLayout>,
        ratio: f32,
    },
}

impl PaneLayout {
    pub fn pane_ids(&self) -> Vec<PaneId> {
        match self {
            PaneLayout::Single(id) => vec![id.clone()],
            PaneLayout::Horizontal { left, right, .. } => {
                let mut ids = left.pane_ids();
                ids.extend(right.pane_ids());
                ids
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                let mut ids = top.pane_ids();
                ids.extend(bottom.pane_ids());
                ids
            }
        }
    }

    pub fn pane_count(&self) -> usize {
        self.pane_ids().len()
    }
}

// Keep TabLayout as alias for compatibility
pub type TabLayout = PaneLayout;
// Keep TabId for compatibility with tab_titles() in gpui_entry
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub String);

/// Terminal manager — layout tree of panes, each pane has its own tabs
pub struct TerminalManager {
    layout: PaneLayout,
    panes: HashMap<PaneId, TerminalPane>,
    active_pane: PaneId,
    next_pane_num: usize,
}

impl TerminalManager {
    pub fn new() -> Self {
        let pane_id = PaneId("pane-1".to_string());
        let pane = TerminalPane::new(pane_id.clone());
        let mut panes = HashMap::new();
        panes.insert(pane_id.clone(), pane);

        Self {
            layout: PaneLayout::Single(pane_id.clone()),
            panes,
            active_pane: pane_id,
            next_pane_num: 2,
        }
    }

    fn next_pane_id(&mut self) -> PaneId {
        let id = PaneId(format!("pane-{}", self.next_pane_num));
        self.next_pane_num += 1;
        id
    }

    // === Active pane/terminal access ===

    pub fn active_terminal(&mut self) -> Option<&mut TerminalView> {
        let pane = self.panes.get_mut(&self.active_pane)?;
        Some(pane.active_terminal())
    }

    pub fn active_terminal_ref(&self) -> Option<&TerminalView> {
        let pane = self.panes.get(&self.active_pane)?;
        Some(pane.active_terminal_ref())
    }

    pub fn active_pane_id(&self) -> Option<&PaneId> {
        Some(&self.active_pane)
    }

    pub fn set_active_pane(&mut self, pane_id: &PaneId) {
        if self.panes.contains_key(pane_id) {
            self.active_pane = pane_id.clone();
        }
    }

    pub fn get_pane(&self, pane_id: &PaneId) -> Option<&TerminalPane> {
        self.panes.get(pane_id)
    }

    pub fn get_pane_mut(&mut self, pane_id: &PaneId) -> Option<&mut TerminalPane> {
        self.panes.get_mut(pane_id)
    }

    // === Resize terminals ===

    /// Resize all terminals in a pane to the given pixel dimensions
    pub fn resize_pane_terminals(&mut self, pane_id: &PaneId, width_px: f32, height_px: f32, cell_w: f32, cell_h: f32) {
        let tab_strip_h = 28.0_f32; // approximate tab strip height
        let padding = 8.0_f32; // terminal padding (p_1 = 4px each side)
        let cols = ((width_px - padding) / cell_w).floor().max(1.0) as usize;
        let rows = ((height_px - tab_strip_h - padding) / cell_h).floor().max(1.0) as usize;
        if let Some(pane) = self.panes.get_mut(pane_id) {
            for tab in &mut pane.tabs {
                let (cur_cols, cur_rows) = tab.terminal.emulator().dimensions();
                if cur_cols != cols || cur_rows != rows {
                    let _ = tab.terminal.resize(cols, rows);
                }
            }
        }
    }

    /// Resize all panes based on layout and available space
    pub fn resize_all_panes(&mut self, avail_w: f32, avail_h: f32, cell_w: f32, cell_h: f32) {
        if let Some(layout) = self.active_layout().cloned() {
            let sizes = Self::compute_pane_sizes(&layout, avail_w, avail_h);
            for (pane_id, w, h) in sizes {
                self.resize_pane_terminals(&pane_id, w, h, cell_w, cell_h);
            }
        }
    }

    fn compute_pane_sizes(layout: &PaneLayout, w: f32, h: f32) -> Vec<(PaneId, f32, f32)> {
        match layout {
            PaneLayout::Single(id) => vec![(id.clone(), w, h)],
            PaneLayout::Horizontal { left, right, ratio } => {
                let handle = 6.0_f32;
                let usable = (w - handle).max(0.0);
                let lw = usable * ratio;
                let rw = usable * (1.0 - ratio);
                let mut sizes = Self::compute_pane_sizes(left, lw, h);
                sizes.extend(Self::compute_pane_sizes(right, rw, h));
                sizes
            }
            PaneLayout::Vertical { top, bottom, ratio } => {
                let handle = 6.0_f32;
                let usable = (h - handle).max(0.0);
                let th = usable * ratio;
                let bh = usable * (1.0 - ratio);
                let mut sizes = Self::compute_pane_sizes(top, w, th);
                sizes.extend(Self::compute_pane_sizes(bottom, w, bh));
                sizes
            }
        }
    }

    // === Spawn ===

    pub fn spawn_in_active(&mut self, profile: amux_core::TerminalLaunchProfile) -> Result<(), String> {
        let term = self.active_terminal().ok_or("no active terminal")?;
        term.spawn(profile)
    }

    // === Tab operations (per-pane) ===

    /// Add a new tab to the active pane and spawn a PTY
    pub fn add_tab_to_active_pane(&mut self, title: String) -> Option<usize> {
        let pane = self.panes.get_mut(&self.active_pane)?;
        Some(pane.add_tab(title))
    }

    /// Close the active tab in the active pane
    pub fn close_active_tab(&mut self) -> bool {
        let pane = match self.panes.get_mut(&self.active_pane) {
            Some(p) => p,
            None => return false,
        };
        pane.close_tab(pane.active_tab)
    }

    /// Switch to a specific tab in the active pane
    pub fn set_active_tab_in_pane(&mut self, tab_index: usize) {
        if let Some(pane) = self.panes.get_mut(&self.active_pane) {
            if tab_index < pane.tabs.len() {
                pane.active_tab = tab_index;
            }
        }
    }

    // === Split operations ===

    pub fn split_active_pane(&mut self, direction: SplitDirection) -> Option<PaneId> {
        let target = self.active_pane.clone();
        if !self.panes.contains_key(&target) {
            return None;
        }

        let new_pane_id = self.next_pane_id();
        let new_pane = TerminalPane::new(new_pane_id.clone());
        self.panes.insert(new_pane_id.clone(), new_pane);

        Self::split_in_layout(&mut self.layout, &target, &new_pane_id, direction);
        self.active_pane = new_pane_id.clone();
        Some(new_pane_id)
    }

    fn split_in_layout(
        layout: &mut PaneLayout,
        target: &PaneId,
        new_id: &PaneId,
        direction: SplitDirection,
    ) -> bool {
        match layout {
            PaneLayout::Single(id) if id == target => {
                let original = PaneLayout::Single(target.clone());
                let new_node = PaneLayout::Single(new_id.clone());
                *layout = match direction {
                    SplitDirection::Horizontal => PaneLayout::Horizontal {
                        left: Box::new(original),
                        right: Box::new(new_node),
                        ratio: 0.5,
                    },
                    SplitDirection::Vertical => PaneLayout::Vertical {
                        top: Box::new(original),
                        bottom: Box::new(new_node),
                        ratio: 0.5,
                    },
                };
                true
            }
            PaneLayout::Single(_) => false,
            PaneLayout::Horizontal { left, right, .. } => {
                Self::split_in_layout(left, target, new_id, direction)
                    || Self::split_in_layout(right, target, new_id, direction)
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                Self::split_in_layout(top, target, new_id, direction)
                    || Self::split_in_layout(bottom, target, new_id, direction)
            }
        }
    }

    // === Close pane ===

    pub fn close_active_pane(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }
        let target = self.active_pane.clone();
        Self::remove_from_layout(&mut self.layout, &target);
        self.panes.remove(&target);
        self.active_pane = Self::first_pane(&self.layout)
            .unwrap_or_else(|| self.panes.keys().next().cloned().unwrap());
        true
    }

    fn remove_from_layout(layout: &mut PaneLayout, target: &PaneId) -> bool {
        match layout {
            PaneLayout::Single(_) => false,
            PaneLayout::Horizontal { left, right, .. } => {
                if matches!(left.as_ref(), PaneLayout::Single(id) if id == target) {
                    *layout = *right.clone();
                    return true;
                }
                if matches!(right.as_ref(), PaneLayout::Single(id) if id == target) {
                    *layout = *left.clone();
                    return true;
                }
                Self::remove_from_layout(left, target)
                    || Self::remove_from_layout(right, target)
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                if matches!(top.as_ref(), PaneLayout::Single(id) if id == target) {
                    *layout = *bottom.clone();
                    return true;
                }
                if matches!(bottom.as_ref(), PaneLayout::Single(id) if id == target) {
                    *layout = *top.clone();
                    return true;
                }
                Self::remove_from_layout(top, target)
                    || Self::remove_from_layout(bottom, target)
            }
        }
    }

    fn first_pane(layout: &PaneLayout) -> Option<PaneId> {
        match layout {
            PaneLayout::Single(id) => Some(id.clone()),
            PaneLayout::Horizontal { left, .. } => Self::first_pane(left),
            PaneLayout::Vertical { top, .. } => Self::first_pane(top),
        }
    }

    // === Polling ===

    pub fn poll_all(&mut self) -> bool {
        let mut any = false;
        for pane in self.panes.values_mut() {
            if pane.poll_all_tabs() {
                any = true;
            }
        }
        any
    }

    // === Resize ===

    /// Update the split ratio for a split identified by the first pane in its left/top child
    pub fn update_split_ratio(&mut self, first_pane_id: &PaneId, new_ratio: f32) {
        Self::update_ratio_in_layout(&mut self.layout, first_pane_id, new_ratio);
    }

    fn update_ratio_in_layout(layout: &mut PaneLayout, target_second: &PaneId, new_ratio: f32) -> bool {
        match layout {
            PaneLayout::Single(_) => false,
            PaneLayout::Horizontal { left, right, ratio } => {
                if Self::first_pane(right).as_ref() == Some(target_second) {
                    *ratio = new_ratio.clamp(0.1, 0.9);
                    return true;
                }
                Self::update_ratio_in_layout(left, target_second, new_ratio)
                    || Self::update_ratio_in_layout(right, target_second, new_ratio)
            }
            PaneLayout::Vertical { top, bottom, ratio } => {
                if Self::first_pane(bottom).as_ref() == Some(target_second) {
                    *ratio = new_ratio.clamp(0.1, 0.9);
                    return true;
                }
                Self::update_ratio_in_layout(top, target_second, new_ratio)
                    || Self::update_ratio_in_layout(bottom, target_second, new_ratio)
            }
        }
    }

    // === Layout / query ===

    pub fn active_layout(&self) -> Option<&PaneLayout> {
        Some(&self.layout)
    }

    pub fn total_panes(&self) -> usize {
        self.panes.len()
    }

    pub fn total_tabs(&self) -> usize {
        self.panes.values().map(|p| p.tab_count()).sum()
    }

    /// Global tab titles (for bottom bar — shows pane count summary)
    pub fn tab_titles(&self) -> Vec<(TabId, String, bool)> {
        // Not used anymore — per-pane tabs replace this
        vec![]
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_pane() {
        let mut manager = TerminalManager::new();
        assert_eq!(manager.total_panes(), 1);

        let new_pane = manager.split_active_pane(SplitDirection::Horizontal);
        assert!(new_pane.is_some());
        assert_eq!(manager.total_panes(), 2);
    }

    #[test]
    fn test_nested_split() {
        let mut manager = TerminalManager::new();
        manager.split_active_pane(SplitDirection::Horizontal);
        assert_eq!(manager.total_panes(), 2);

        // Split the new active pane again
        manager.split_active_pane(SplitDirection::Vertical);
        assert_eq!(manager.total_panes(), 3);
    }

    #[test]
    fn test_close_pane() {
        let mut manager = TerminalManager::new();
        manager.split_active_pane(SplitDirection::Horizontal);
        assert_eq!(manager.total_panes(), 2);

        assert!(manager.close_active_pane());
        assert_eq!(manager.total_panes(), 1);
    }

    #[test]
    fn test_per_pane_tabs() {
        let mut manager = TerminalManager::new();
        // Add a second tab to the active pane
        manager.add_tab_to_active_pane("Tab 2".into());

        let pane = manager.get_pane(&manager.active_pane).unwrap();
        assert_eq!(pane.tab_count(), 2);
        assert_eq!(pane.active_tab, 1); // New tab is active
    }

    #[test]
    fn test_close_tab() {
        let mut manager = TerminalManager::new();
        manager.add_tab_to_active_pane("Tab 2".into());
        assert_eq!(manager.get_pane(&manager.active_pane).unwrap().tab_count(), 2);

        assert!(manager.close_active_tab());
        assert_eq!(manager.get_pane(&manager.active_pane).unwrap().tab_count(), 1);
    }
}
