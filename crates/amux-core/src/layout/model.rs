use crate::{PaneId, TabId, TabState};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LayoutNode {
    Split(SplitNode),
    Pane(PaneNode),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SplitNode {
    pub id: String,
    pub axis: SplitAxis,
    pub ratio: f32,
    pub first: Box<LayoutNode>,
    pub second: Box<LayoutNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaneNode {
    pub pane_id: PaneId,
    pub tabs: Vec<TabState>,
    pub active_tab_id: TabId,
}

impl LayoutNode {
    pub fn single_pane(pane: PaneNode) -> Self {
        Self::Pane(pane)
    }
    
    /// Resize a split node by adjusting its ratio
    /// delta is the amount to change the ratio by (positive = increase first pane)
    pub fn resize_split(&mut self, split_id: &str, delta: f32) -> bool {
        self.resize_split_inner(split_id, delta)
    }
    
    fn resize_split_inner(&mut self, split_id: &str, delta: f32) -> bool {
        match self {
            LayoutNode::Split(split) if split.id == split_id => {
                split.ratio = Self::clamp_ratio(split.ratio + delta);
                true
            }
            LayoutNode::Split(split) => {
                split.first.resize_split_inner(split_id, delta)
                    || split.second.resize_split_inner(split_id, delta)
            }
            LayoutNode::Pane(_) => false,
        }
    }
    
    /// Reset all split ratios to 0.5 (equal distribution)
    pub fn reset_split_ratios(&mut self) {
        self.reset_ratios_inner()
    }
    
    fn reset_ratios_inner(&mut self) {
        match self {
            LayoutNode::Split(split) => {
                split.ratio = 0.5;
                split.first.reset_ratios_inner();
                split.second.reset_ratios_inner();
            }
            LayoutNode::Pane(_) => {}
        }
    }
    
    fn clamp_ratio(ratio: f32) -> f32 {
        const MIN: f32 = 0.1;
        const MAX: f32 = 0.9;
        ratio.max(MIN).min(MAX)
    }
}

impl PaneNode {
    pub fn new(pane_id: PaneId, tabs: Vec<TabState>, active_tab_id: TabId) -> Self {
        Self {
            pane_id,
            tabs,
            active_tab_id,
        }
    }

    pub fn with_tab(pane_id: PaneId, tab: TabState) -> Self {
        Self {
            pane_id,
            active_tab_id: tab.id.clone(),
            tabs: vec![tab],
        }
    }
}
