//! Hierarchy browser component for navigating waveform scopes and signals

use std::collections::{HashMap, HashSet};

use ratatui::prelude::*;
use ratatui::widgets::Block;
use tui_tree_widget::{Tree, TreeItem, TreeState};

use fstty_core::hierarchy::Hierarchy;
use fstty_core::types::{ScopeId, ScopeType, VarDirection, VarId};

/// How a node is selected for export
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectionMode {
    /// Scope + all descendants; for vars: simply selected
    Recursive,
    /// Only this scope's direct vars (scopes only)
    ScopeOnly,
}

/// Result of toggling a node's selection
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleResult {
    Selected(SelectionMode),
    Deselected,
    NoSelection,
}

/// Compute the next selection state for a node.
/// Scopes cycle: None → Recursive → ScopeOnly → None
/// Vars cycle:   None → Recursive → None
fn next_selection_state(current: Option<SelectionMode>, is_scope: bool) -> Option<SelectionMode> {
    match (current, is_scope) {
        (None, _) => Some(SelectionMode::Recursive),
        (Some(SelectionMode::Recursive), true) => Some(SelectionMode::ScopeOnly),
        (Some(SelectionMode::Recursive), false) => None,
        (Some(SelectionMode::ScopeOnly), _) => None,
    }
}

/// All available scope types for filtering
pub const ALL_SCOPE_TYPES: &[(ScopeType, &str, &str)] = &[
    // Verilog/SystemVerilog
    (ScopeType::Module, "Module", "Verilog/SV module"),
    (ScopeType::Generate, "Generate", "Generate block"),
    (ScopeType::Interface, "Interface", "SV interface"),
    (ScopeType::Task, "Task", "Task block"),
    (ScopeType::Function, "Function", "Function block"),
    (ScopeType::Begin, "Begin", "Named begin block"),
    (ScopeType::Fork, "Fork", "Fork block"),
    (ScopeType::Package, "Package", "SV package"),
    (ScopeType::Program, "Program", "SV program"),
    (ScopeType::Class, "Class", "SV class"),
    (ScopeType::Struct, "Struct", "SV struct"),
    (ScopeType::Union, "Union", "SV union"),
];

/// Identifier for tree nodes
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum NodeId {
    #[default]
    Root,
    /// Scope node
    Scope(ScopeId),
    /// Variable node
    Var(VarId),
}

/// Filter configuration for the hierarchy browser
#[derive(Clone, Debug)]
pub struct FilterConfig {
    /// Scope types to show (navigation hierarchy)
    scope_types: HashSet<ScopeType>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        // Default: only show navigable hierarchy (modules, generates, interfaces, begin blocks)
        let mut scope_types = HashSet::new();
        scope_types.insert(ScopeType::Module);
        scope_types.insert(ScopeType::Generate);
        scope_types.insert(ScopeType::Interface);
        scope_types.insert(ScopeType::Begin);
        Self { scope_types }
    }
}

impl FilterConfig {
    /// Check if a scope type should be shown
    pub fn allows_scope(&self, scope_type: ScopeType) -> bool {
        self.scope_types.contains(&scope_type)
    }

    /// Toggle a scope type on/off
    pub fn toggle_scope_type(&mut self, scope_type: ScopeType) {
        if self.scope_types.contains(&scope_type) {
            self.scope_types.remove(&scope_type);
        } else {
            self.scope_types.insert(scope_type);
        }
    }

    /// Check if a scope type is enabled
    pub fn is_scope_enabled(&self, scope_type: ScopeType) -> bool {
        self.scope_types.contains(&scope_type)
    }

    /// Enable all scope types
    pub fn enable_all_scopes(&mut self) {
        for (scope_type, _, _) in ALL_SCOPE_TYPES {
            self.scope_types.insert(*scope_type);
        }
    }

    /// Disable all scope types
    pub fn disable_all_scopes(&mut self) {
        self.scope_types.clear();
    }

    /// Reset to default (navigable hierarchy only)
    pub fn reset_to_default(&mut self) {
        *self = Self::default();
    }
}

/// Hierarchy browser state
pub struct HierarchyBrowser {
    /// Tree widget state (selection, expanded nodes)
    state: TreeState<NodeId>,
    /// Our own tracking of expanded scope IDs (for lazy loading)
    expanded: HashSet<NodeId>,
    /// Scopes where signals should be shown (toggled with 's')
    show_signals: HashSet<NodeId>,
    /// Selected signals/scopes for export (toggled with Space)
    selected_for_export: HashMap<NodeId, SelectionMode>,
    /// Filter configuration
    filter: FilterConfig,
}

impl Default for HierarchyBrowser {
    fn default() -> Self {
        Self::new()
    }
}

impl HierarchyBrowser {
    /// Create a new hierarchy browser
    pub fn new() -> Self {
        Self {
            state: TreeState::default(),
            expanded: HashSet::new(),
            show_signals: HashSet::new(),
            selected_for_export: HashMap::new(),
            filter: FilterConfig::default(),
        }
    }

    /// Reset the browser state (e.g., when loading a new file)
    pub fn reset(&mut self) {
        self.state = TreeState::default();
        self.expanded.clear();
        self.show_signals.clear();
        self.selected_for_export.clear();
        // Keep filter config - user probably wants same settings for new file
    }

    /// Get the currently selected node ID
    pub fn selected(&self) -> Option<NodeId> {
        self.state.selected().last().copied()
    }

    /// Toggle signal visibility for the currently selected scope
    /// Returns true if signals are now shown, false if hidden
    pub fn toggle_show_signals(&mut self) -> Option<bool> {
        if let Some(selected) = self.selected() {
            // Only works on scopes, not variables
            if matches!(selected, NodeId::Scope(_)) {
                if self.show_signals.contains(&selected) {
                    self.show_signals.remove(&selected);
                    Some(false)
                } else {
                    self.show_signals.insert(selected);
                    // Also expand the node so signals become visible
                    self.expanded.insert(selected);
                    Some(true)
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if a scope has signal visibility enabled
    pub fn is_showing_signals(&self, node_id: &NodeId) -> bool {
        self.show_signals.contains(node_id)
    }

    /// Toggle selection of the currently highlighted node for export.
    /// Scopes cycle: None → Recursive → ScopeOnly → None
    /// Vars cycle:   None → Recursive → None
    pub fn toggle_selection(&mut self) -> ToggleResult {
        let node_id = match self.selected() {
            Some(id) => id,
            None => return ToggleResult::NoSelection,
        };
        let is_scope = matches!(node_id, NodeId::Scope(_));
        let current = self.selected_for_export.get(&node_id).copied();
        match next_selection_state(current, is_scope) {
            Some(mode) => {
                self.selected_for_export.insert(node_id, mode);
                ToggleResult::Selected(mode)
            }
            None => {
                self.selected_for_export.remove(&node_id);
                ToggleResult::Deselected
            }
        }
    }

    /// Get the selection mode for a node, if any.
    pub fn selection_mode(&self, node_id: &NodeId) -> Option<SelectionMode> {
        self.selected_for_export.get(node_id).copied()
    }

    /// Get count of selected items
    pub fn selection_count(&self) -> usize {
        self.selected_for_export.len()
    }

    /// Clear all selections
    pub fn clear_selection(&mut self) {
        self.selected_for_export.clear();
    }

    /// Get all selected node IDs with their selection modes (for export).
    pub fn selected_nodes(&self) -> &HashMap<NodeId, SelectionMode> {
        &self.selected_for_export
    }

    /// Get mutable access to the filter config
    pub fn filter_mut(&mut self) -> &mut FilterConfig {
        &mut self.filter
    }

    /// Get read access to the filter config
    pub fn filter(&self) -> &FilterConfig {
        &self.filter
    }

    /// Rebuild the tree (clear expanded state, keep filter and show_signals)
    pub fn rebuild(&mut self) {
        self.state = TreeState::default();
        self.expanded.clear();
        // Keep filter and show_signals - user wants to see same signals with new filter
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

        for &scope_id in hierarchy.top_scopes() {
            if let Some(item) = self.build_scope_item(hierarchy, scope_id, false) {
                items.push(item);
            }
        }

        items
    }

    /// Build a tree item for a scope.
    /// `ancestor_recursive` is true when a parent scope is selected as Recursive.
    fn build_scope_item<'a>(
        &self,
        hierarchy: &'a Hierarchy,
        scope_id: ScopeId,
        ancestor_recursive: bool,
    ) -> Option<TreeItem<'a, NodeId>> {
        let scope_type = hierarchy.scope_type(scope_id);

        // Filter: skip scopes that don't match our filter config
        if !self.filter.allows_scope(scope_type) {
            return None;
        }

        let name = hierarchy.scope_name(scope_id);
        let node_id = NodeId::Scope(scope_id);

        // Check if signals should be shown for this scope
        let show_signals_here = self.show_signals.contains(&node_id);
        // Check selection mode
        let mode = self.selected_for_export.get(&node_id).copied();

        // Format label - add selection indicator
        let selected_marker = match mode {
            Some(SelectionMode::Recursive) => "● ",
            Some(SelectionMode::ScopeOnly) => "○ ",
            None if ancestor_recursive => "● ",
            None => "",
        };
        let signals_marker = if show_signals_here { " *" } else { "" };
        let label = format!("{}{} ({:?}){}", selected_marker, name, scope_type, signals_marker);

        // Propagate: children inherit ancestor_recursive if this scope is Recursive
        let child_ancestor_recursive =
            ancestor_recursive || mode == Some(SelectionMode::Recursive);

        // Check if this scope has visible children
        let has_child_scopes = hierarchy.scope_children(scope_id).iter()
            .any(|&child_id| self.filter.allows_scope(hierarchy.scope_type(child_id)));
        let has_child_vars = show_signals_here && !hierarchy.scope_vars(scope_id).is_empty();
        let has_children = has_child_scopes || has_child_vars;

        if !has_children {
            // Leaf scope (no visible children)
            return Some(TreeItem::new_leaf(node_id, label));
        }

        // Check if we've expanded this node
        if self.expanded.contains(&node_id) {
            // Build children
            let mut children = Vec::new();

            // Child scopes (filtered)
            for &child_id in hierarchy.scope_children(scope_id) {
                if let Some(child_item) = self.build_scope_item(hierarchy, child_id, child_ancestor_recursive) {
                    children.push(child_item);
                }
            }

            // Child variables (only if "show signals" is enabled for this scope)
            if show_signals_here {
                for &var_id in hierarchy.scope_vars(scope_id) {
                    if let Some(var_item) = self.build_var_item(hierarchy, var_id, child_ancestor_recursive) {
                        children.push(var_item);
                    }
                }
            }

            if children.is_empty() {
                // All children were filtered out
                Some(TreeItem::new_leaf(node_id, label))
            } else {
                TreeItem::new(node_id, label, children).ok()
            }
        } else {
            // Not expanded - create a placeholder child so the widget shows expand arrow
            let placeholder = TreeItem::new_leaf(NodeId::Root, "...");
            TreeItem::new(node_id, label, vec![placeholder]).ok()
        }
    }

    /// Build a tree item for a variable.
    /// `ancestor_recursive` is true when a parent scope is selected as Recursive.
    fn build_var_item<'a>(
        &self,
        hierarchy: &'a Hierarchy,
        var_id: VarId,
        ancestor_recursive: bool,
    ) -> Option<TreeItem<'a, NodeId>> {
        let name = hierarchy.var_name(var_id);
        let width = hierarchy.var_width(var_id);
        let direction = hierarchy.var_direction(var_id);
        let node_id = NodeId::Var(var_id);

        // Check selection mode
        let mode = self.selected_for_export.get(&node_id).copied();
        let selected_marker = match mode {
            Some(_) => "● ",
            None if ancestor_recursive => "● ",
            None => "",
        };

        // Format: "name [width]" with direction indicator
        let dir_indicator = match direction {
            VarDirection::Input => "->",
            VarDirection::Output => "<-",
            VarDirection::InOut => "<>",
            _ => "  ",
        };

        let label = if width == 0 {
            format!("{}{} {}", selected_marker, dir_indicator, name)
        } else {
            format!("{}{} {} [{}]", selected_marker, dir_indicator, name, width)
        };

        Some(TreeItem::new_leaf(node_id, label))
    }

    /// Render the hierarchy browser
    pub fn render(&mut self, frame: &mut Frame, area: Rect, hierarchy: &Hierarchy, block: Block) {
        // Render the block first and get the inner area (respects padding)
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Build and render the tree in the inner area
        let items = self.build_tree_items(hierarchy);

        let tree = Tree::new(&items)
            .expect("tree items should be valid")
            .highlight_style(Style::default().reversed())
            .node_closed_symbol("▶ ")
            .node_open_symbol("▼ ")
            .node_no_children_symbol("  ");

        frame.render_stateful_widget(tree, inner, &mut self.state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fstty_core::hierarchy::{HierarchyBuilder, HierarchyEvent};
    use fstty_core::types::{VarDirection, VarType};

    /// Build a small hierarchy and return (hierarchy, top_scope_id, child_scope_id, var_ids)
    /// Structure:
    ///   top (Module)              scope 0
    ///     ├── var_a (signal 1)    var 0
    ///     └── child (Module)      scope 1
    ///           └── var_b         var 1
    fn test_hierarchy() -> fstty_core::hierarchy::Hierarchy {
        let mut b = HierarchyBuilder::new();
        b.event(HierarchyEvent::EnterScope {
            name: "top".into(),
            scope_type: ScopeType::Module,
        });
        b.event(HierarchyEvent::Var {
            name: "var_a".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: fstty_core::types::SignalId::from_raw(1),
            is_alias: false,
        });
        b.event(HierarchyEvent::EnterScope {
            name: "child".into(),
            scope_type: ScopeType::Module,
        });
        b.event(HierarchyEvent::Var {
            name: "var_b".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: fstty_core::types::SignalId::from_raw(2),
            is_alias: false,
        });
        b.event(HierarchyEvent::ExitScope);
        b.event(HierarchyEvent::ExitScope);
        b.build()
    }

    #[test]
    fn next_state_scope_cycles_recursive_scope_only_none() {
        // None → Recursive
        assert_eq!(next_selection_state(None, true), Some(SelectionMode::Recursive));
        // Recursive → ScopeOnly
        assert_eq!(
            next_selection_state(Some(SelectionMode::Recursive), true),
            Some(SelectionMode::ScopeOnly)
        );
        // ScopeOnly → None
        assert_eq!(next_selection_state(Some(SelectionMode::ScopeOnly), true), None);
    }

    #[test]
    fn next_state_var_cycles_recursive_none() {
        // None → Recursive
        assert_eq!(next_selection_state(None, false), Some(SelectionMode::Recursive));
        // Recursive → None (skips ScopeOnly)
        assert_eq!(next_selection_state(Some(SelectionMode::Recursive), false), None);
    }

    #[test]
    fn selection_mode_returns_correct_state() {
        let h = test_hierarchy();
        let top_scope = *h.top_scopes().first().unwrap();
        let var = h.scope_vars(top_scope)[0];

        let mut browser = HierarchyBrowser::new();
        let scope_node = NodeId::Scope(top_scope);
        let var_node = NodeId::Var(var);

        assert_eq!(browser.selection_mode(&scope_node), None);
        assert_eq!(browser.selection_mode(&var_node), None);

        browser.selected_for_export.insert(scope_node, SelectionMode::Recursive);
        assert_eq!(browser.selection_mode(&scope_node), Some(SelectionMode::Recursive));

        browser.selected_for_export.insert(scope_node, SelectionMode::ScopeOnly);
        assert_eq!(browser.selection_mode(&scope_node), Some(SelectionMode::ScopeOnly));

        browser.selected_for_export.insert(var_node, SelectionMode::Recursive);
        assert_eq!(browser.selection_mode(&var_node), Some(SelectionMode::Recursive));
    }

    #[test]
    fn selection_count_reflects_map_size() {
        let h = test_hierarchy();
        let top_scope = *h.top_scopes().first().unwrap();
        let var = h.scope_vars(top_scope)[0];

        let mut browser = HierarchyBrowser::new();
        assert_eq!(browser.selection_count(), 0);

        browser.selected_for_export.insert(NodeId::Scope(top_scope), SelectionMode::Recursive);
        assert_eq!(browser.selection_count(), 1);

        browser.selected_for_export.insert(NodeId::Var(var), SelectionMode::Recursive);
        assert_eq!(browser.selection_count(), 2);
    }

    #[test]
    fn clear_selection_empties_map() {
        let h = test_hierarchy();
        let top_scope = *h.top_scopes().first().unwrap();
        let child_scope = h.scope_children(top_scope)[0];

        let mut browser = HierarchyBrowser::new();
        browser.selected_for_export.insert(NodeId::Scope(top_scope), SelectionMode::Recursive);
        browser.selected_for_export.insert(NodeId::Scope(child_scope), SelectionMode::ScopeOnly);
        assert_eq!(browser.selection_count(), 2);

        browser.clear_selection();
        assert_eq!(browser.selection_count(), 0);
    }
}
