//! Terminal Manager — per-pane tabs + nested splits (limux-style)
//!
//! Each pane has its own tab strip. Panes can be split arbitrarily.
//! Uses AlacrittyTerminal for full VT100/xterm escape sequence support.

use std::collections::HashMap;

use serde::{Serialize, Deserialize};
use crate::terminal::alacritty_view::AlacrittyTerminal;

/// Unique ID for a pane
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    pub terminal: Option<AlacrittyTerminal>,
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
                terminal: None,
            }],
            active_tab: 0,
        }
    }

    /// Get the active terminal
    pub fn active_terminal(&mut self) -> Option<&mut AlacrittyTerminal> {
        self.tabs.get_mut(self.active_tab)?.terminal.as_mut()
    }

    /// Get the active terminal (immutable)
    pub fn active_terminal_ref(&self) -> Option<&AlacrittyTerminal> {
        self.tabs.get(self.active_tab)?.terminal.as_ref()
    }

    /// Add a new tab to this pane and make it active
    pub fn add_tab(&mut self, title: String) -> usize {
        self.tabs.push(PaneTab {
            title,
            terminal: None,
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
                let title = t.terminal.as_ref()
                    .and_then(|term| term.title())
                    .unwrap_or_else(|| t.title.clone());
                (i, title, i == self.active_tab)
            })
            .collect()
    }

    /// Set active tab by index
    pub fn set_active_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }
}

/// Pane layout tree — splits of panes
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub String);

/// Serializable layout state for persistence
#[derive(Serialize, Deserialize)]
struct LayoutState {
    layout: PaneLayout,
    active_pane: PaneId,
    next_pane_num: usize,
}

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

    pub fn active_terminal(&mut self) -> Option<&mut AlacrittyTerminal> {
        let pane = self.panes.get_mut(&self.active_pane)?;
        pane.active_terminal()
    }

    pub fn active_terminal_ref(&self) -> Option<&AlacrittyTerminal> {
        let pane = self.panes.get(&self.active_pane)?;
        pane.active_terminal_ref()
    }

    pub fn active_pane_id(&self) -> Option<&PaneId> {
        Some(&self.active_pane)
    }

    pub fn set_active_pane(&mut self, pane_id: &PaneId) {
        if self.panes.contains_key(pane_id) {
            self.active_pane = pane_id.clone();
        }
    }

    pub fn set_active_tab_in_pane(&mut self, tab_index: usize) {
        if let Some(pane) = self.panes.get_mut(&self.active_pane) {
            pane.set_active_tab(tab_index);
        }
    }

    pub fn get_pane(&self, pane_id: &PaneId) -> Option<&TerminalPane> {
        self.panes.get(pane_id)
    }

    pub fn get_pane_mut(&mut self, pane_id: &PaneId) -> Option<&mut TerminalPane> {
        self.panes.get_mut(pane_id)
    }

    // === Resize terminals ===

    pub fn resize_pane_terminals(&mut self, pane_id: &PaneId, width_px: f32, height_px: f32, cell_w: f32, cell_h: f32) {
        let tab_strip_h = 28.0_f32;
        let padding = 8.0_f32;
        let cols = ((width_px - padding) / cell_w).floor().max(1.0) as u16;
        let rows = ((height_px - tab_strip_h - padding) / cell_h).floor().max(1.0) as u16;
        if let Some(pane) = self.panes.get_mut(pane_id) {
            for tab in &mut pane.tabs {
                if let Some(ref mut term) = tab.terminal {
                    let (cur_cols, cur_rows) = term.dimensions();
                    if cur_cols != cols || cur_rows != rows {
                        term.resize(cols, rows);
                    }
                }
            }
        }
    }

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

    /// Spawn a terminal in the active pane's active tab using AlacrittyTerminal
    pub fn spawn_in_active(&mut self, shell: &str, args: &[String], cwd: Option<&str>) -> Result<(), String> {
        self.spawn_in_pane(&self.active_pane.clone(), shell, args, cwd)
    }

    /// Spawn a terminal in a specific pane's active tab
    pub fn spawn_in_pane(&mut self, pane_id: &PaneId, shell: &str, args: &[String], cwd: Option<&str>) -> Result<(), String> {
        let pane = self.panes.get_mut(pane_id).ok_or("pane not found")?;
        let tab = pane.tabs.get_mut(pane.active_tab).ok_or("no active tab")?;
        if tab.terminal.is_some() {
            return Ok(()); // already has a terminal
        }
        let term = AlacrittyTerminal::new(120, 40, 8, 20, shell, args, cwd)?;
        tab.terminal = Some(term);
        Ok(())
    }

    // === Tab operations (per-pane) ===

    pub fn add_tab_to_active_pane(&mut self, title: String) -> Option<usize> {
        let pane = self.panes.get_mut(&self.active_pane)?;
        Some(pane.add_tab(title))
    }

    pub fn close_active_tab(&mut self) -> bool {
        let pane = self.panes.get_mut(&self.active_pane);
        match pane {
            Some(pane) => pane.close_tab(pane.active_tab),
            None => false,
        }
    }

    // === Split ===

    pub fn split_active_pane(&mut self, direction: SplitDirection) {
        let new_pane_id = self.next_pane_id();
        let new_pane = TerminalPane::new(new_pane_id.clone());
        self.panes.insert(new_pane_id.clone(), new_pane);

        let active = self.active_pane.clone();
        Self::split_in_layout(&mut self.layout, &active, &new_pane_id, direction);
        self.active_pane = new_pane_id;
    }

    fn split_in_layout(layout: &mut PaneLayout, target: &PaneId, new_pane: &PaneId, direction: SplitDirection) -> bool {
        match layout {
            PaneLayout::Single(id) if id == target => {
                let old = std::mem::replace(layout, PaneLayout::Single(PaneId("temp".to_string())));
                *layout = match direction {
                    SplitDirection::Horizontal => PaneLayout::Horizontal {
                        left: Box::new(old),
                        right: Box::new(PaneLayout::Single(new_pane.clone())),
                        ratio: 0.5,
                    },
                    SplitDirection::Vertical => PaneLayout::Vertical {
                        top: Box::new(old),
                        bottom: Box::new(PaneLayout::Single(new_pane.clone())),
                        ratio: 0.5,
                    },
                };
                true
            }
            PaneLayout::Horizontal { left, right, .. } => {
                Self::split_in_layout(left, target, new_pane, direction)
                    || Self::split_in_layout(right, target, new_pane, direction)
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                Self::split_in_layout(top, target, new_pane, direction)
                    || Self::split_in_layout(bottom, target, new_pane, direction)
            }
            _ => false,
        }
    }

    // === Close pane ===

    pub fn close_active_pane(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }
        let closed = self.active_pane.clone();
        if Self::remove_from_layout(&mut self.layout, &closed) {
            self.panes.remove(&closed);
            self.active_pane = Self::first_pane(&self.layout)
                .or_else(|| self.panes.keys().next().cloned())
                .unwrap_or_else(|| PaneId("pane-1".to_string()));
            true
        } else {
            false
        }
    }

    fn remove_from_layout(layout: &mut PaneLayout, target: &PaneId) -> bool {
        match layout {
            PaneLayout::Horizontal { left, right, .. } => {
                if matches!(**left, PaneLayout::Single(ref id) if id == target) {
                    *layout = *right.clone();
                    return true;
                }
                if matches!(**right, PaneLayout::Single(ref id) if id == target) {
                    *layout = *left.clone();
                    return true;
                }
                Self::remove_from_layout(left, target)
                    || Self::remove_from_layout(right, target)
            }
            PaneLayout::Vertical { top, bottom, .. } => {
                if matches!(**top, PaneLayout::Single(ref id) if id == target) {
                    *layout = *bottom.clone();
                    return true;
                }
                if matches!(**bottom, PaneLayout::Single(ref id) if id == target) {
                    *layout = *top.clone();
                    return true;
                }
                Self::remove_from_layout(top, target)
                    || Self::remove_from_layout(bottom, target)
            }
            _ => false,
        }
    }

    // === Move tab between panes ===

    /// Move a tab from one pane to another.
    /// If the source pane becomes empty, it is removed from the layout.
    /// Returns true if the move was successful.
    pub fn move_tab_to_pane(
        &mut self,
        source_pane: &PaneId,
        tab_index: usize,
        target_pane: &PaneId,
    ) -> bool {
        if source_pane == target_pane {
            return false;
        }
        // Validate both panes exist and tab index is valid
        let src_tab_count = match self.panes.get(source_pane) {
            Some(p) => p.tabs.len(),
            None => return false,
        };
        if tab_index >= src_tab_count {
            return false;
        }
        if !self.panes.contains_key(target_pane) {
            return false;
        }

        // Remove tab from source
        let src = match self.panes.get_mut(source_pane) {
            Some(p) => p,
            None => return false,
        };
        let tab = src.tabs.remove(tab_index);
        if src.active_tab >= src.tabs.len() && !src.tabs.is_empty() {
            src.active_tab = src.tabs.len() - 1;
        }

        // Add tab to target and make it active
        let target = match self.panes.get_mut(target_pane) {
            Some(p) => p,
            None => return false,
        };
        target.tabs.push(tab);
        target.active_tab = target.tabs.len() - 1;

        // If source pane is now empty, remove it from layout
        let source_empty = self.panes.get(source_pane).map_or(true, |p| p.tabs.is_empty());
        if source_empty {
            Self::remove_from_layout(&mut self.layout, source_pane);
            self.panes.remove(source_pane);
            // If the closed pane was active, switch to target
            if &self.active_pane == source_pane {
                self.active_pane = target_pane.clone();
            }
        }

        // Make target the active pane
        self.active_pane = target_pane.clone();
        true
    }

    // === Resize ===

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

    pub fn pane_iter(&self) -> impl Iterator<Item = (&PaneId, &TerminalPane)> {
        self.panes.iter()
    }

    pub fn total_panes(&self) -> usize {
        self.panes.len()
    }

    pub fn total_tabs(&self) -> usize {
        self.panes.values().map(|p| p.tab_count()).sum()
    }

    pub fn tab_titles(&self) -> Vec<(TabId, String, bool)> {
        vec![]
    }

    fn first_pane(layout: &PaneLayout) -> Option<PaneId> {
        match layout {
            PaneLayout::Single(id) => Some(id.clone()),
            PaneLayout::Horizontal { left, .. } => Self::first_pane(left),
            PaneLayout::Vertical { top, .. } => Self::first_pane(top),
        }
    }

    // === Polling (no longer needed — alacritty has its own event loop) ===

    pub fn poll_all(&mut self) -> bool {
        false
    }

    // === Layout persistence ===

    /// Serialize the current layout to JSON
    pub fn save_layout(&self) -> String {
        let state = LayoutState {
            layout: self.layout.clone(),
            active_pane: self.active_pane.clone(),
            next_pane_num: self.next_pane_num,
        };
        serde_json::to_string(&state).unwrap_or_default()
    }

    /// Restore layout from JSON, creating empty panes for each pane ID
    pub fn restore_layout(json: &str) -> Option<Self> {
        let state: LayoutState = serde_json::from_str(json).ok()?;
        let pane_ids = state.layout.pane_ids();
        if pane_ids.is_empty() {
            return None;
        }
        let mut panes = HashMap::new();
        for id in &pane_ids {
            panes.insert(id.clone(), TerminalPane::new(id.clone()));
        }
        // Validate active_pane exists in restored layout, fallback to first pane
        let active_pane = if pane_ids.contains(&state.active_pane) {
            state.active_pane
        } else {
            pane_ids[0].clone()
        };
        Some(Self {
            layout: state.layout,
            panes,
            active_pane,
            next_pane_num: state.next_pane_num,
        })
    }
}
