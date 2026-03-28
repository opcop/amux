use crate::{Direction, LayoutNode, PaneId, PaneNode, SplitAxis, SplitNode, TabId, TabState};

pub fn active_pane_exists(layout: &LayoutNode, pane_id: &PaneId) -> bool {
    match layout {
        LayoutNode::Pane(pane) => &pane.pane_id == pane_id,
        LayoutNode::Split(split) => {
            active_pane_exists(&split.first, pane_id) || active_pane_exists(&split.second, pane_id)
        }
    }
}

pub fn find_pane_mut<'a>(layout: &'a mut LayoutNode, pane_id: &PaneId) -> Option<&'a mut PaneNode> {
    match layout {
        LayoutNode::Pane(pane) if &pane.pane_id == pane_id => Some(pane),
        LayoutNode::Pane(_) => None,
        LayoutNode::Split(split) => find_pane_mut(&mut split.first, pane_id)
            .or_else(|| find_pane_mut(&mut split.second, pane_id)),
    }
}

pub fn append_tab(layout: &mut LayoutNode, pane_id: &PaneId, tab: TabState) -> bool {
    let Some(pane) = find_pane_mut(layout, pane_id) else {
        return false;
    };
    pane.active_tab_id = tab.id.clone();
    pane.tabs.push(tab);
    true
}

pub fn activate_tab(layout: &mut LayoutNode, pane_id: &PaneId, tab_id: &TabId) -> bool {
    let Some(pane) = find_pane_mut(layout, pane_id) else {
        return false;
    };
    if pane.tabs.iter().any(|tab| &tab.id == tab_id) {
        pane.active_tab_id = tab_id.clone();
        true
    } else {
        false
    }
}

pub fn split_pane(
    layout: &mut LayoutNode,
    pane_id: &PaneId,
    axis: SplitAxis,
    split_id: impl Into<String>,
    new_pane: PaneNode,
) -> bool {
    split_pane_inner(layout, pane_id, axis, split_id.into(), new_pane)
}

fn split_pane_inner(
    layout: &mut LayoutNode,
    pane_id: &PaneId,
    axis: SplitAxis,
    split_id: String,
    new_pane: PaneNode,
) -> bool {
    match layout {
        LayoutNode::Pane(existing) if &existing.pane_id == pane_id => {
            let old = existing.clone();
            *layout = LayoutNode::Split(SplitNode {
                id: split_id,
                axis,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(old)),
                second: Box::new(LayoutNode::Pane(new_pane)),
            });
            true
        }
        LayoutNode::Pane(_) => false,
        LayoutNode::Split(split) => {
            split_pane_inner(
                &mut split.first,
                pane_id,
                axis,
                split_id.clone(),
                new_pane.clone(),
            ) || split_pane_inner(&mut split.second, pane_id, axis, split_id, new_pane)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseTabOutcome {
    TabClosed,
    PaneRemoved,
    NotFound,
    CannotRemoveLastTab,
}

pub fn close_tab(layout: &mut LayoutNode, pane_id: &PaneId, tab_id: &TabId) -> CloseTabOutcome {
    close_tab_inner(layout, pane_id, tab_id, true)
}

fn close_tab_inner(
    layout: &mut LayoutNode,
    pane_id: &PaneId,
    tab_id: &TabId,
    is_root: bool,
) -> CloseTabOutcome {
    match layout {
        LayoutNode::Pane(pane) if &pane.pane_id == pane_id => {
            let Some(index) = pane.tabs.iter().position(|tab| &tab.id == tab_id) else {
                return CloseTabOutcome::NotFound;
            };

            if pane.tabs.len() == 1 {
                if is_root {
                    return CloseTabOutcome::CannotRemoveLastTab;
                }
                pane.tabs.remove(index);
                return CloseTabOutcome::PaneRemoved;
            }

            let removed_active = pane.active_tab_id == *tab_id;
            pane.tabs.remove(index);
            if removed_active {
                let fallback_index = index.saturating_sub(1).min(pane.tabs.len() - 1);
                pane.active_tab_id = pane.tabs[fallback_index].id.clone();
            }
            CloseTabOutcome::TabClosed
        }
        LayoutNode::Pane(_) => CloseTabOutcome::NotFound,
        LayoutNode::Split(split) => {
            let left = close_tab_inner(&mut split.first, pane_id, tab_id, false);
            if left == CloseTabOutcome::PaneRemoved {
                *layout = (*split.second).clone();
                return CloseTabOutcome::PaneRemoved;
            }
            if left != CloseTabOutcome::NotFound {
                return left;
            }

            let right = close_tab_inner(&mut split.second, pane_id, tab_id, false);
            if right == CloseTabOutcome::PaneRemoved {
                *layout = (*split.first).clone();
                return CloseTabOutcome::PaneRemoved;
            }
            right
        }
    }
}

/// Find the split that contains a given pane
/// Returns the split_id and the axis
pub fn find_split_for_pane(layout: &LayoutNode, pane_id: &PaneId) -> Option<(String, SplitAxis)> {
    match layout {
        LayoutNode::Split(split) => {
            if contains_pane(&split.first, pane_id) || contains_pane(&split.second, pane_id) {
                Some((split.id.clone(), split.axis))
            } else {
                find_split_for_pane(&split.first, pane_id)
                    .or_else(|| find_split_for_pane(&split.second, pane_id))
            }
        }
        LayoutNode::Pane(_) => None,
    }
}

fn contains_pane(layout: &LayoutNode, pane_id: &PaneId) -> bool {
    match layout {
        LayoutNode::Pane(pane) => &pane.pane_id == pane_id,
        LayoutNode::Split(split) => {
            contains_pane(&split.first, pane_id) || contains_pane(&split.second, pane_id)
        }
    }
}

/// Get all splits in the layout with their current ratios
pub fn get_all_splits(layout: &LayoutNode) -> Vec<(String, SplitAxis, f32)> {
    let mut splits = Vec::new();
    collect_splits(layout, &mut splits);
    splits
}

fn collect_splits(layout: &LayoutNode, splits: &mut Vec<(String, SplitAxis, f32)>) {
    match layout {
        LayoutNode::Split(split) => {
            splits.push((split.id.clone(), split.axis, split.ratio));
            collect_splits(&split.first, splits);
            collect_splits(&split.second, splits);
        }
        LayoutNode::Pane(_) => {}
    }
}

/// Get all panes in the layout in order (left-to-right, top-to-bottom)
pub fn get_all_panes(layout: &LayoutNode) -> Vec<PaneNode> {
    let mut panes = Vec::new();
    collect_panes(layout, &mut panes);
    panes
}

fn collect_panes(layout: &LayoutNode, panes: &mut Vec<PaneNode>) {
    match layout {
        LayoutNode::Pane(pane) => {
            panes.push(pane.clone());
        }
        LayoutNode::Split(split) => {
            collect_panes(&split.first, panes);
            collect_panes(&split.second, panes);
        }
    }
}

/// Find the pane in the given direction from the current pane
/// Returns the pane_id of the target pane, or None if no pane exists in that direction
pub fn focus_pane_in_direction(
    layout: &LayoutNode,
    current: &PaneId,
    direction: Direction,
) -> Option<PaneId> {
    let all_panes = get_all_panes(layout);
    let current_idx = all_panes.iter().position(|p| &p.pane_id == current)?;

    match direction {
        Direction::Left | Direction::Up => {
            if current_idx > 0 {
                Some(all_panes[current_idx - 1].pane_id.clone())
            } else {
                None
            }
        }
        Direction::Right | Direction::Down => {
            if current_idx + 1 < all_panes.len() {
                Some(all_panes[current_idx + 1].pane_id.clone())
            } else {
                None
            }
        }
    }
}

/// Find the first pane in the layout (for initial focus)
pub fn find_first_pane(layout: &LayoutNode) -> Option<PaneId> {
    find_first_pane_inner(layout)
}

fn find_first_pane_inner(layout: &LayoutNode) -> Option<PaneId> {
    match layout {
        LayoutNode::Pane(pane) => Some(pane.pane_id.clone()),
        LayoutNode::Split(split) => find_first_pane_inner(&split.first),
    }
}

/// Find the last pane in the layout
pub fn find_last_pane(layout: &LayoutNode) -> Option<PaneId> {
    find_last_pane_inner(layout)
}

fn find_last_pane_inner(layout: &LayoutNode) -> Option<PaneId> {
    match layout {
        LayoutNode::Pane(pane) => Some(pane.pane_id.clone()),
        LayoutNode::Split(split) => find_last_pane_inner(&split.second),
    }
}

/// Close a pane and all its tabs
/// Returns the pane that should gain focus after closing
pub fn close_pane(layout: &mut LayoutNode, pane_id: &PaneId) -> ClosePaneOutcome {
    close_pane_inner(layout, pane_id, true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosePaneOutcome {
    PaneClosed,
    PaneRemoved,
    CannotRemoveLastPane,
}

fn close_pane_inner(layout: &mut LayoutNode, pane_id: &PaneId, is_root: bool) -> ClosePaneOutcome {
    match layout {
        LayoutNode::Pane(pane) if &pane.pane_id == pane_id => {
            if is_root {
                ClosePaneOutcome::CannotRemoveLastPane
            } else {
                ClosePaneOutcome::PaneRemoved
            }
        }
        LayoutNode::Pane(_) => ClosePaneOutcome::PaneClosed,
        LayoutNode::Split(split) => {
            let left_result = close_pane_inner(&mut split.first, pane_id, false);
            if left_result == ClosePaneOutcome::PaneRemoved {
                *layout = (*split.second).clone();
                return ClosePaneOutcome::PaneRemoved;
            }
            if left_result != ClosePaneOutcome::PaneClosed {
                return left_result;
            }

            let right_result = close_pane_inner(&mut split.second, pane_id, false);
            if right_result == ClosePaneOutcome::PaneRemoved {
                *layout = (*split.first).clone();
                return ClosePaneOutcome::PaneRemoved;
            }
            right_result
        }
    }
}

/// Get the next tab in the same pane
pub fn focus_next_tab(layout: &LayoutNode, pane_id: &PaneId, current: &TabId) -> Option<TabId> {
    let pane = find_pane(layout, pane_id)?;
    let current_idx = pane.tabs.iter().position(|t| &t.id == current)?;
    let next_idx = (current_idx + 1) % pane.tabs.len();
    Some(pane.tabs[next_idx].id.clone())
}

/// Get the previous tab in the same pane
pub fn focus_previous_tab(layout: &LayoutNode, pane_id: &PaneId, current: &TabId) -> Option<TabId> {
    let pane = find_pane(layout, pane_id)?;
    let current_idx = pane.tabs.iter().position(|t| &t.id == current)?;
    let prev_idx = if current_idx == 0 {
        pane.tabs.len() - 1
    } else {
        current_idx - 1
    };
    Some(pane.tabs[prev_idx].id.clone())
}

/// Find a pane node (immutable)
pub fn find_pane<'a>(layout: &'a LayoutNode, pane_id: &PaneId) -> Option<&'a PaneNode> {
    find_pane_inner(layout, pane_id)
}

fn find_pane_inner<'a>(layout: &'a LayoutNode, pane_id: &PaneId) -> Option<&'a PaneNode> {
    match layout {
        LayoutNode::Pane(pane) if &pane.pane_id == pane_id => Some(pane),
        LayoutNode::Pane(_) => None,
        LayoutNode::Split(split) => find_pane_inner(&split.first, pane_id)
            .or_else(|| find_pane_inner(&split.second, pane_id)),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        PaneNode, PreviewKind, PreviewSurfaceState, SplitAxis, SurfaceId, SurfaceState, TabId,
        TabState,
    };

    use super::{
        activate_tab, append_tab, close_tab, split_pane, CloseTabOutcome, LayoutNode, PaneId,
    };

    fn preview_tab(id: &str) -> TabState {
        TabState::new(
            TabId::new(id),
            format!("Tab {id}"),
            false,
            SurfaceState::Preview(PreviewSurfaceState {
                surface_id: SurfaceId::new(format!("surface-{id}")),
                source_relative_path: format!("{id}.md"),
                kind: PreviewKind::Markdown,
            }),
        )
    }

    #[test]
    fn append_tab_updates_active_tab() {
        let first = preview_tab("one");
        let second = preview_tab("two");
        let pane_id = PaneId::new("pane-1");
        let mut layout = LayoutNode::single_pane(PaneNode::with_tab(pane_id.clone(), first));

        assert!(append_tab(&mut layout, &pane_id, second.clone()));

        let LayoutNode::Pane(pane) = layout else {
            panic!("expected pane");
        };
        assert_eq!(pane.tabs.len(), 2);
        assert_eq!(pane.active_tab_id, second.id);
    }

    #[test]
    fn split_pane_wraps_existing_and_new_panes() {
        let pane_id = PaneId::new("pane-1");
        let mut layout =
            LayoutNode::single_pane(PaneNode::with_tab(pane_id.clone(), preview_tab("one")));

        assert!(split_pane(
            &mut layout,
            &pane_id,
            SplitAxis::Vertical,
            "split-1",
            PaneNode::with_tab(PaneId::new("pane-2"), preview_tab("two")),
        ));

        let LayoutNode::Split(split) = layout else {
            panic!("expected split");
        };
        assert_eq!(split.axis, SplitAxis::Vertical);
    }

    #[test]
    fn close_tab_collapses_empty_non_root_pane() {
        let left_pane_id = PaneId::new("pane-1");
        let right_pane_id = PaneId::new("pane-2");
        let mut layout = LayoutNode::Split(crate::SplitNode {
            id: "split-1".into(),
            axis: SplitAxis::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Pane(PaneNode::with_tab(
                left_pane_id.clone(),
                preview_tab("one"),
            ))),
            second: Box::new(LayoutNode::Pane(PaneNode::with_tab(
                right_pane_id,
                preview_tab("two"),
            ))),
        });

        let outcome = close_tab(&mut layout, &left_pane_id, &TabId::new("one"));
        assert_eq!(outcome, CloseTabOutcome::PaneRemoved);

        let LayoutNode::Pane(pane) = layout else {
            panic!("expected collapsed pane");
        };
        assert_eq!(pane.active_tab_id, TabId::new("two"));
    }

    #[test]
    fn close_tab_rejects_last_root_tab() {
        let pane_id = PaneId::new("pane-1");
        let mut layout =
            LayoutNode::single_pane(PaneNode::with_tab(pane_id.clone(), preview_tab("one")));

        let outcome = close_tab(&mut layout, &pane_id, &TabId::new("one"));
        assert_eq!(outcome, CloseTabOutcome::CannotRemoveLastTab);
    }

    #[test]
    fn activate_tab_switches_active_tab() {
        let first = preview_tab("one");
        let second = preview_tab("two");
        let pane_id = PaneId::new("pane-1");
        let mut layout = LayoutNode::single_pane(PaneNode::new(
            pane_id.clone(),
            vec![first.clone(), second.clone()],
            first.id.clone(),
        ));

        assert!(activate_tab(&mut layout, &pane_id, &second.id));

        let LayoutNode::Pane(pane) = layout else {
            panic!("expected pane");
        };
        assert_eq!(pane.active_tab_id, second.id);
    }
}
