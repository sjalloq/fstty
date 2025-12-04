//! Hierarchy browser component for navigating waveform scopes and signals

use std::collections::HashSet;
use std::num::NonZeroU32;

use ratatui::prelude::*;
use ratatui::widgets::Block;
use tui_tree_widget::{Tree, TreeItem, TreeState};
use wellen::Hierarchy;

/// Identifier for tree nodes
/// Uses raw indices since wellen's ScopeRef/VarRef don't implement Hash
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum NodeId {
    #[default]
    Root,
    /// Scope with its raw index
    Scope(NonZeroU32),
    /// Variable with its raw index
    Var(NonZeroU32),
}

impl NodeId {
    fn from_scope(scope_ref: wellen::ScopeRef) -> Self {
        // ScopeRef is a newtype over NonZeroU32, we need to extract the raw value
        // Using unsafe transmute since there's no public accessor
        let raw: NonZeroU32 = unsafe { std::mem::transmute(scope_ref) };
        NodeId::Scope(raw)
    }

    fn from_var(var_ref: wellen::VarRef) -> Self {
        let raw: NonZeroU32 = unsafe { std::mem::transmute(var_ref) };
        NodeId::Var(raw)
    }

    fn to_scope_ref(self) -> Option<wellen::ScopeRef> {
        match self {
            NodeId::Scope(raw) => Some(unsafe { std::mem::transmute(raw) }),
            _ => None,
        }
    }
}

/// Hierarchy browser state
pub struct HierarchyBrowser {
    /// Tree widget state (selection, expanded nodes)
    state: TreeState<NodeId>,
    /// Our own tracking of expanded scope IDs (for lazy loading)
    expanded: HashSet<NodeId>,
}

impl HierarchyBrowser {
    /// Create a new hierarchy browser
    pub fn new() -> Self {
        Self {
            state: TreeState::default(),
            expanded: HashSet::new(),
        }
    }

    /// Reset the browser state (e.g., when loading a new file)
    pub fn reset(&mut self) {
        self.state = TreeState::default();
        self.expanded.clear();
    }

    /// Navigate up
    pub fn up(&mut self) {
        self.state.key_up();
    }

    /// Navigate down
    pub fn down(&mut self) {
        self.state.key_down();
    }

    /// Collapse current node or go to parent
    pub fn left(&mut self) {
        // Get currently selected before the action
        if let Some(selected) = self.state.selected().last().copied() {
            if self.expanded.contains(&selected) {
                self.expanded.remove(&selected);
            }
        }
        self.state.key_left();
    }

    /// Expand current node or go to first child
    pub fn right(&mut self) {
        // Get currently selected and expand it
        if let Some(selected) = self.state.selected().last().copied() {
            self.expanded.insert(selected);
        }
        self.state.key_right();
    }

    /// Toggle expand/collapse of selected node
    pub fn toggle(&mut self) {
        if let Some(selected) = self.state.selected().last().copied() {
            if self.expanded.contains(&selected) {
                self.expanded.remove(&selected);
            } else {
                self.expanded.insert(selected);
            }
        }
        self.state.toggle_selected();
    }

    /// Build tree items from the hierarchy (lazy - only builds expanded nodes)
    fn build_tree_items<'a>(&self, hierarchy: &'a Hierarchy) -> Vec<TreeItem<'a, NodeId>> {
        let mut items = Vec::new();

        // Add top-level scopes
        for scope_ref in hierarchy.scopes() {
            if let Some(item) = self.build_scope_item(hierarchy, scope_ref) {
                items.push(item);
            }
        }

        // Add top-level variables
        for var_ref in hierarchy.vars() {
            if let Some(item) = self.build_var_item(hierarchy, var_ref) {
                items.push(item);
            }
        }

        items
    }

    /// Build a tree item for a scope
    fn build_scope_item<'a>(
        &self,
        hierarchy: &'a Hierarchy,
        scope_ref: wellen::ScopeRef,
    ) -> Option<TreeItem<'a, NodeId>> {
        let scope = &hierarchy[scope_ref];
        let name = scope.name(hierarchy);
        let scope_type = scope.scope_type();

        // Format: "name (Type)"
        let label = format!("{} ({:?})", name, scope_type);
        let node_id = NodeId::from_scope(scope_ref);

        // Check if this scope has children
        let has_child_scopes = scope.scopes(hierarchy).next().is_some();
        let has_child_vars = scope.vars(hierarchy).next().is_some();
        let has_children = has_child_scopes || has_child_vars;

        if !has_children {
            // Leaf scope (rare but possible)
            return Some(TreeItem::new_leaf(node_id, label));
        }

        // Check if we've expanded this node
        if self.expanded.contains(&node_id) {
            // Build children
            let mut children = Vec::new();

            // Child scopes
            for child_ref in scope.scopes(hierarchy) {
                if let Some(child_item) = self.build_scope_item(hierarchy, child_ref) {
                    children.push(child_item);
                }
            }

            // Child variables
            for var_ref in scope.vars(hierarchy) {
                if let Some(var_item) = self.build_var_item(hierarchy, var_ref) {
                    children.push(var_item);
                }
            }

            TreeItem::new(node_id, label, children).ok()
        } else {
            // Not expanded - create a placeholder child so the widget shows expand arrow
            // The placeholder will be replaced when user expands
            let placeholder = TreeItem::new_leaf(NodeId::Root, "...");
            TreeItem::new(node_id, label, vec![placeholder]).ok()
        }
    }

    /// Build a tree item for a variable
    fn build_var_item<'a>(
        &self,
        hierarchy: &'a Hierarchy,
        var_ref: wellen::VarRef,
    ) -> Option<TreeItem<'a, NodeId>> {
        let var = &hierarchy[var_ref];
        let name = var.name(hierarchy);
        let width = var.length().map(|l| l.to_string()).unwrap_or_default();

        // Format: "name [width]" or just "name"
        let label = if width.is_empty() {
            name.to_string()
        } else {
            format!("{} [{}]", name, width)
        };

        Some(TreeItem::new_leaf(NodeId::from_var(var_ref), label))
    }

    /// Render the hierarchy browser
    pub fn render(&mut self, frame: &mut Frame, area: Rect, hierarchy: &Hierarchy, block: Block) {
        let items = self.build_tree_items(hierarchy);

        let tree = Tree::new(&items)
            .expect("tree items should be valid")
            .block(block)
            .highlight_style(Style::default().reversed())
            .node_closed_symbol("▶ ")
            .node_open_symbol("▼ ")
            .node_no_children_symbol("  ");

        frame.render_stateful_widget(tree, area, &mut self.state);
    }
}
