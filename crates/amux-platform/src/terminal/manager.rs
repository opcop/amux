//! Terminal Manager - manages multiple terminal tabs and panes
//! 
//! This module provides:
//! - Multiple terminal tabs
//! - Terminal pane splitting
//! - Active tab/pane tracking

use std::collections::HashMap;

use crate::terminal::view::TerminalView;

/// Unique ID for a terminal tab
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub String);

/// Unique ID for a pane within a tab
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(pub String);

/// Split direction
#[derive(Clone, Debug, Copy)]
pub enum SplitDirection {
    Horizontal,  // Side by side
    Vertical,    // Top and bottom
}

/// A single pane containing a terminal
pub struct TerminalPane {
    pub id: PaneId,
    pub terminal: TerminalView,
}

impl TerminalPane {
    pub fn new(id: PaneId, terminal: TerminalView) -> Self {
        Self { id, terminal }
    }
}

/// Tab layout - simple horizontal or vertical split
#[derive(Clone, Debug)]
pub enum TabLayout {
    /// Single pane
    Single(PaneId),
    /// Horizontal split (left/right)
    Horizontal { left: Box<TabLayout>, right: Box<TabLayout>, ratio: f32 },
    /// Vertical split (top/bottom)
    Vertical { top: Box<TabLayout>, bottom: Box<TabLayout>, ratio: f32 },
}

impl TabLayout {
    /// Get all pane IDs in this layout
    pub fn pane_ids(&self) -> Vec<PaneId> {
        match self {
            TabLayout::Single(id) => vec![id.clone()],
            TabLayout::Horizontal { left, right, .. } => {
                let mut ids = left.pane_ids();
                ids.extend(right.pane_ids());
                ids
            }
            TabLayout::Vertical { top, bottom, .. } => {
                let mut ids = top.pane_ids();
                ids.extend(bottom.pane_ids());
                ids
            }
        }
    }

    /// Count panes in this layout
    pub fn pane_count(&self) -> usize {
        match self {
            TabLayout::Single(_) => 1,
            TabLayout::Horizontal { left, right, .. } => left.pane_count() + right.pane_count(),
            TabLayout::Vertical { top, bottom, .. } => top.pane_count() + bottom.pane_count(),
        }
    }

    /// Find a pane ID by position (0-based index among all panes)
    pub fn pane_at_index(&self, index: usize) -> Option<PaneId> {
        let ids = self.pane_ids();
        ids.get(index).cloned()
    }
}

/// A tab containing terminal panes
pub struct TerminalTab {
    pub id: TabId,
    pub title: String,
    pub layout: TabLayout,
    pub panes: HashMap<PaneId, TerminalPane>,
    pub active_pane: PaneId,
}

impl TerminalTab {
    pub fn new(id: TabId, title: String, pane_id: PaneId) -> Self {
        let terminal = TerminalView::new();
        let pane = TerminalPane::new(pane_id.clone(), terminal);
        let mut panes = HashMap::new();
        panes.insert(pane_id.clone(), pane);
        
        Self {
            id,
            title,
            layout: TabLayout::Single(pane_id.clone()),
            panes,
            active_pane: pane_id,
        }
    }

    /// Split a pane in this tab
    pub fn split_pane(&mut self, pane_id: &PaneId, direction: SplitDirection) -> Option<PaneId> {
        // Check if pane exists
        if !self.panes.contains_key(pane_id) {
            return None;
        }
        
        // Generate new pane ID
        let new_pane_id = PaneId(format!("{}-split-{}", self.id.0, self.panes.len()));
        
        // Create new terminal
        let terminal = TerminalView::new();
        let new_pane = TerminalPane::new(new_pane_id.clone(), terminal);
        self.panes.insert(new_pane_id.clone(), new_pane);
        
        // Update layout
        self.layout = match direction {
            SplitDirection::Horizontal => {
                TabLayout::Horizontal {
                    left: Box::new(TabLayout::Single(pane_id.clone())),
                    right: Box::new(TabLayout::Single(new_pane_id.clone())),
                    ratio: 0.5,
                }
            }
            SplitDirection::Vertical => {
                TabLayout::Vertical {
                    top: Box::new(TabLayout::Single(pane_id.clone())),
                    bottom: Box::new(TabLayout::Single(new_pane_id.clone())),
                    ratio: 0.5,
                }
            }
        };
        
        self.active_pane = new_pane_id.clone();
        Some(new_pane_id)
    }

    /// Get pane by ID
    pub fn get_pane(&self, id: &PaneId) -> Option<&TerminalPane> {
        self.panes.get(id)
    }

    /// Get pane by ID (mutable)
    pub fn get_pane_mut(&mut self, id: &PaneId) -> Option<&mut TerminalPane> {
        self.panes.get_mut(id)
    }
}

/// Terminal manager that handles multiple tabs and panes
pub struct TerminalManager {
    /// All terminal tabs
    tabs: HashMap<TabId, TerminalTab>,
    /// Tab order (for ordering)
    tab_order: Vec<TabId>,
    /// Active tab ID
    active_tab: TabId,
    /// Next tab number for auto-naming
    next_tab_num: usize,
}

impl TerminalManager {
    /// Create a new terminal manager
    pub fn new() -> Self {
        let mut manager = Self {
            tabs: HashMap::new(),
            tab_order: Vec::new(),
            active_tab: TabId(String::new()),
            next_tab_num: 1,
        };
        
        // Create initial tab
        manager.create_tab("Terminal 1".to_string());
        
        manager
    }

    /// Create a new tab
    pub fn create_tab(&mut self, title: String) -> TabId {
        let id = TabId(format!("tab-{}", self.next_tab_num));
        self.next_tab_num += 1;
        
        let pane_id = PaneId(format!("{}-pane-0", id.0));
        let tab = TerminalTab::new(id.clone(), title, pane_id);
        
        self.tabs.insert(id.clone(), tab);
        self.tab_order.push(id.clone());
        self.active_tab = id.clone();
        
        id
    }

    /// Create a new tab with auto-generated name
    pub fn create_tab_auto(&mut self) -> TabId {
        let title = format!("Terminal {}", self.next_tab_num);
        self.create_tab(title)
    }

    /// Close a tab
    pub fn close_tab(&mut self, tab_id: &TabId) -> bool {
        if self.tabs.len() <= 1 {
            return false; // Don't close last tab
        }
        
        let pos = self.tab_order.iter().position(|t| t == tab_id);
        if let Some(pos) = pos {
            self.tab_order.remove(pos);
            self.tabs.remove(tab_id);
            
            // Update active tab if needed
            if self.active_tab == *tab_id {
                self.active_tab = self.tab_order.last().cloned().unwrap_or(TabId(String::new()));
            }
            
            true
        } else {
            false
        }
    }

    /// Get the active tab
    pub fn active_tab(&self) -> Option<&TerminalTab> {
        self.tabs.get(&self.active_tab)
    }

    /// Get the active tab (mutable)
    pub fn active_tab_mut(&mut self) -> Option<&mut TerminalTab> {
        self.tabs.get_mut(&self.active_tab)
    }

    /// Set active tab
    pub fn set_active_tab(&mut self, tab_id: &TabId) {
        if self.tabs.contains_key(tab_id) {
            self.active_tab = tab_id.clone();
        }
    }

    /// Get active terminal
    pub fn active_terminal(&mut self) -> Option<&mut TerminalView> {
        let tab = self.active_tab_mut()?;
        let pane_id = tab.active_pane.clone();
        tab.get_pane_mut(&pane_id).map(|p| &mut p.terminal)
    }

    /// Get active terminal (immutable)
    pub fn active_terminal_ref(&self) -> Option<&TerminalView> {
        let tab = self.active_tab()?;
        tab.get_pane(&tab.active_pane).map(|p| &p.terminal)
    }

    /// Split the active pane
    pub fn split_active_pane(&mut self, direction: SplitDirection) -> Option<PaneId> {
        let tab = self.active_tab_mut()?;
        let pane_id = tab.active_pane.clone();
        tab.split_pane(&pane_id, direction)
    }

    /// Close the active pane
    pub fn close_active_pane(&mut self) -> bool {
        let tab = match self.active_tab_mut() {
            Some(t) => t,
            None => return false,
        };
        
        // Can't close if only one pane
        if tab.panes.len() <= 1 {
            return false;
        }
        
        let pane_id = tab.active_pane.clone();
        
        // Remove the pane
        tab.panes.remove(&pane_id);
        
        // Set new active pane
        let new_active = tab.panes.keys().next().cloned().unwrap_or(PaneId(String::new()));
        tab.active_pane = new_active.clone();
        tab.layout = TabLayout::Single(new_active);
        
        true
    }

    /// Set active pane
    pub fn set_active_pane(&mut self, pane_id: &PaneId) {
        if let Some(tab) = self.active_tab_mut() {
            if tab.panes.contains_key(pane_id) {
                tab.active_pane = pane_id.clone();
            }
        }
    }

    /// Get all tabs
    pub fn tabs(&self) -> impl Iterator<Item = &TerminalTab> {
        self.tab_order.iter().filter_map(|id| self.tabs.get(id))
    }

    /// Get tab count
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Get total pane count
    pub fn total_panes(&self) -> usize {
        self.tabs.values().map(|t| t.panes.len()).sum()
    }

    /// Poll the active terminal for new PTY output and feed it to the emulator.
    /// Returns true if new data was processed.
    pub fn poll_active(&mut self) -> bool {
        if let Some(term) = self.active_terminal() {
            term.poll()
        } else {
            false
        }
    }

    /// Get tab titles for UI
    pub fn tab_titles(&self) -> Vec<(TabId, String, bool)> {
        self.tab_order.iter()
            .filter_map(|id| {
                self.tabs.get(id).map(|tab| {
                    (id.clone(), tab.title.clone(), *id == self.active_tab)
                })
            })
            .collect()
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
    fn test_create_tabs() {
        let mut manager = TerminalManager::new();
        assert_eq!(manager.tab_count(), 1);
        
        let tab2 = manager.create_tab_auto();
        assert_eq!(manager.tab_count(), 2);
        assert!(manager.tabs.contains_key(&tab2));
    }

    #[test]
    fn test_split_pane() {
        let mut manager = TerminalManager::new();
        assert_eq!(manager.total_panes(), 1);
        
        let new_pane = manager.split_active_pane(SplitDirection::Horizontal);
        assert!(new_pane.is_some());
        assert_eq!(manager.total_panes(), 2);
    }
    
    #[test]
    fn test_close_pane() {
        let mut manager = TerminalManager::new();
        manager.split_active_pane(SplitDirection::Horizontal);
        assert_eq!(manager.total_panes(), 2);
        
        assert!(manager.close_active_pane());
        assert_eq!(manager.total_panes(), 1);
    }
}
