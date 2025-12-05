//! Hierarchy browser component for navigating waveform scopes and signals

use std::collections::HashSet;
use std::num::NonZeroU32;

use ratatui::prelude::*;
use ratatui::widgets::Block;
use tui_tree_widget::{Tree, TreeItem, TreeState};
use wellen::{Hierarchy, ScopeType};

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
    // VHDL
    (ScopeType::VhdlArchitecture, "VHDL Arch", "VHDL architecture"),
    (ScopeType::VhdlBlock, "VHDL Block", "VHDL block"),
    (ScopeType::VhdlGenerate, "VHDL Gen", "VHDL generate"),
    (ScopeType::VhdlForGenerate, "VHDL For", "VHDL for generate"),
    (ScopeType::VhdlIfGenerate, "VHDL If", "VHDL if generate"),
    (ScopeType::VhdlProcess, "VHDL Proc", "VHDL process"),
    (ScopeType::VhdlFunction, "VHDL Func", "VHDL function"),
    (ScopeType::VhdlProcedure, "VHDL Procedure", "VHDL procedure"),
    (ScopeType::VhdlPackage, "VHDL Pkg", "VHDL package"),
    (ScopeType::VhdlRecord, "VHDL Rec", "VHDL record"),
    (ScopeType::VhdlArray, "VHDL Arr", "VHDL array"),
    // Other
    (ScopeType::GhwGeneric, "GHW Generic", "GHW generic"),
    (ScopeType::Unknown, "Unknown", "Unknown scope type"),
];

/// Convert ScopeType to a discriminant we can hash
/// (ScopeType doesn't implement Hash)
fn scope_type_id(st: ScopeType) -> u8 {
    // Use the discriminant - ScopeType is a simple enum
    // This is safe since we're just getting an integer representation
    unsafe { *(&st as *const ScopeType as *const u8) }
}

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

/// Filter configuration for the hierarchy browser
#[derive(Clone, Debug)]
pub struct FilterConfig {
    /// Scope type IDs to show (navigation hierarchy)
    /// Uses u8 discriminants since ScopeType doesn't implement Hash
    scope_type_ids: HashSet<u8>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        // Default: only show navigable hierarchy (modules, generates, interfaces, begin blocks)
        let mut scope_type_ids = HashSet::new();
        scope_type_ids.insert(scope_type_id(ScopeType::Module));
        scope_type_ids.insert(scope_type_id(ScopeType::Generate));
        scope_type_ids.insert(scope_type_id(ScopeType::Interface));
        scope_type_ids.insert(scope_type_id(ScopeType::Begin));
        // Also include VHDL equivalents
        scope_type_ids.insert(scope_type_id(ScopeType::VhdlArchitecture));
        scope_type_ids.insert(scope_type_id(ScopeType::VhdlBlock));
        scope_type_ids.insert(scope_type_id(ScopeType::VhdlForGenerate));
        scope_type_ids.insert(scope_type_id(ScopeType::VhdlIfGenerate));
        scope_type_ids.insert(scope_type_id(ScopeType::VhdlGenerate));
        Self { scope_type_ids }
    }
}

impl FilterConfig {
    /// Check if a scope type should be shown
    pub fn allows_scope(&self, scope_type: ScopeType) -> bool {
        self.scope_type_ids.contains(&scope_type_id(scope_type))
    }

    /// Toggle a scope type on/off
    pub fn toggle_scope_type(&mut self, scope_type: ScopeType) {
        let id = scope_type_id(scope_type);
        if self.scope_type_ids.contains(&id) {
            self.scope_type_ids.remove(&id);
        } else {
            self.scope_type_ids.insert(id);
        }
    }

    /// Check if a scope type is enabled
    pub fn is_scope_enabled(&self, scope_type: ScopeType) -> bool {
        self.scope_type_ids.contains(&scope_type_id(scope_type))
    }

    /// Enable all scope types
    pub fn enable_all_scopes(&mut self) {
        for (scope_type, _, _) in ALL_SCOPE_TYPES {
            self.scope_type_ids.insert(scope_type_id(*scope_type));
        }
    }

    /// Disable all scope types
    pub fn disable_all_scopes(&mut self) {
        self.scope_type_ids.clear();
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
    selected_for_export: HashSet<NodeId>,
    /// Filter configuration
    filter: FilterConfig,
}

impl HierarchyBrowser {
    /// Create a new hierarchy browser
    pub fn new() -> Self {
        Self {
            state: TreeState::default(),
            expanded: HashSet::new(),
            show_signals: HashSet::new(),
            selected_for_export: HashSet::new(),
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

    /// Toggle selection of the currently highlighted node for export
    /// Returns Some(true) if now selected, Some(false) if deselected, None if nothing selected
    pub fn toggle_selection(&mut self) -> Option<bool> {
        if let Some(node_id) = self.selected() {
            if self.selected_for_export.contains(&node_id) {
                self.selected_for_export.remove(&node_id);
                Some(false)
            } else {
                self.selected_for_export.insert(node_id);
                Some(true)
            }
        } else {
            None
        }
    }

    /// Check if a node is selected for export
    pub fn is_selected_for_export(&self, node_id: &NodeId) -> bool {
        self.selected_for_export.contains(node_id)
    }

    /// Get count of selected items
    pub fn selection_count(&self) -> usize {
        self.selected_for_export.len()
    }

    /// Clear all selections
    pub fn clear_selection(&mut self) {
        self.selected_for_export.clear();
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

        // Add top-level scopes (filtered by config)
        for scope_ref in hierarchy.scopes() {
            if let Some(item) = self.build_scope_item(hierarchy, scope_ref, None) {
                items.push(item);
            }
        }

        // Top-level variables are only shown if there's no parent scope
        // (rare, but handle it - these would need a "show all" at root level)

        items
    }

    /// Build a tree item for a scope
    fn build_scope_item<'a>(
        &self,
        hierarchy: &'a Hierarchy,
        scope_ref: wellen::ScopeRef,
        _parent_node_id: Option<NodeId>,
    ) -> Option<TreeItem<'a, NodeId>> {
        let scope = &hierarchy[scope_ref];
        let scope_type = scope.scope_type();

        // Filter: skip scopes that don't match our filter config
        if !self.filter.allows_scope(scope_type) {
            return None;
        }

        let name = scope.name(hierarchy);
        let node_id = NodeId::from_scope(scope_ref);

        // Check if signals should be shown for this scope
        let show_signals_here = self.show_signals.contains(&node_id);
        // Check if selected for export
        let is_selected = self.selected_for_export.contains(&node_id);

        // Format label - add indicators
        let selected_marker = if is_selected { "● " } else { "" };
        let signals_marker = if show_signals_here { " *" } else { "" };
        let label = format!("{}{} ({:?}){}", selected_marker, name, scope_type, signals_marker);

        // Check if this scope has visible children
        let has_child_scopes = scope.scopes(hierarchy)
            .any(|child_ref| {
                let child = &hierarchy[child_ref];
                self.filter.allows_scope(child.scope_type())
            });
        let has_child_vars = show_signals_here && scope.vars(hierarchy).next().is_some();
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
            for child_ref in scope.scopes(hierarchy) {
                if let Some(child_item) = self.build_scope_item(hierarchy, child_ref, Some(node_id)) {
                    children.push(child_item);
                }
            }

            // Child variables (only if "show signals" is enabled for this scope)
            if show_signals_here {
                for var_ref in scope.vars(hierarchy) {
                    if let Some(var_item) = self.build_var_item(hierarchy, var_ref) {
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

    /// Build a tree item for a variable
    fn build_var_item<'a>(
        &self,
        hierarchy: &'a Hierarchy,
        var_ref: wellen::VarRef,
    ) -> Option<TreeItem<'a, NodeId>> {
        let var = &hierarchy[var_ref];
        let name = var.name(hierarchy);
        let width = var.length().map(|l| l.to_string()).unwrap_or_default();
        let direction = var.direction();
        let node_id = NodeId::from_var(var_ref);

        // Check if selected for export
        let is_selected = self.selected_for_export.contains(&node_id);
        let selected_marker = if is_selected { "● " } else { "" };

        // Format: "name [width]" with direction indicator
        let dir_indicator = match direction {
            wellen::VarDirection::Input => "->",
            wellen::VarDirection::Output => "<-",
            wellen::VarDirection::InOut => "<>",
            _ => "  ",
        };

        let label = if width.is_empty() {
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
