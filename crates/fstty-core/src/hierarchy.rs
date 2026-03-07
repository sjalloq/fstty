//! Hierarchy data structure and builder.
//!
//! `Hierarchy` is a concrete, read-only tree of scopes and variables built once
//! when a waveform file is opened. Every backend constructs the same type via
//! `HierarchyBuilder`, which consumes a stream of `HierarchyEvent`s.

use std::collections::HashSet;

use crate::types::{ScopeId, ScopeType, SignalId, VarDirection, VarId, VarType};

// ---------------------------------------------------------------------------
// HierarchyEvent — the builder input
// ---------------------------------------------------------------------------

/// Events emitted by a backend while walking its parsed hierarchy.
#[derive(Debug, Clone)]
pub enum HierarchyEvent {
    EnterScope {
        name: String,
        scope_type: ScopeType,
    },
    ExitScope,
    Var {
        name: String,
        var_type: VarType,
        direction: VarDirection,
        width: u32,
        signal_id: SignalId,
        is_alias: bool,
    },
}

// ---------------------------------------------------------------------------
// Internal storage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ScopeData {
    name: String,
    scope_type: ScopeType,
    parent: Option<ScopeId>,
    children: Vec<ScopeId>,
    vars: Vec<VarId>,
}

#[derive(Debug, Clone)]
struct VarData {
    name: String,
    var_type: VarType,
    direction: VarDirection,
    width: u32,
    signal_id: SignalId,
    parent_scope: Option<ScopeId>,
}

// ---------------------------------------------------------------------------
// Hierarchy
// ---------------------------------------------------------------------------

/// Read-only hierarchy of scopes and variables.
#[derive(Debug, Clone)]
pub struct Hierarchy {
    scopes: Vec<ScopeData>,
    vars: Vec<VarData>,
    top_scopes: Vec<ScopeId>,
    unique_signals: HashSet<SignalId>,
}

impl Hierarchy {
    // -- Navigation --

    /// Top-level scopes (roots of the tree).
    pub fn top_scopes(&self) -> &[ScopeId] {
        &self.top_scopes
    }

    /// Child scopes of a scope.
    pub fn scope_children(&self, id: ScopeId) -> &[ScopeId] {
        &self.scopes[id.0 as usize].children
    }

    /// Variables directly contained in a scope.
    pub fn scope_vars(&self, id: ScopeId) -> &[VarId] {
        &self.scopes[id.0 as usize].vars
    }

    /// Parent scope, or `None` for top-level scopes.
    pub fn scope_parent(&self, id: ScopeId) -> Option<ScopeId> {
        self.scopes[id.0 as usize].parent
    }

    // -- Scope metadata --

    /// Short name of a scope (e.g. `"cpu"`).
    pub fn scope_name(&self, id: ScopeId) -> &str {
        &self.scopes[id.0 as usize].name
    }

    /// Dot-separated full path (e.g. `"top.cpu.core0"`).
    pub fn scope_full_path(&self, id: ScopeId) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(id);
        while let Some(sid) = cur {
            parts.push(self.scope_name(sid));
            cur = self.scope_parent(sid);
        }
        parts.reverse();
        parts.join(".")
    }

    /// Type of a scope.
    pub fn scope_type(&self, id: ScopeId) -> ScopeType {
        self.scopes[id.0 as usize].scope_type
    }

    // -- Variable metadata --

    /// Short name of a variable (e.g. `"clk"`).
    pub fn var_name(&self, id: VarId) -> &str {
        &self.vars[id.0 as usize].name
    }

    /// Dot-separated full path (e.g. `"top.cpu.clk"`).
    pub fn var_full_path(&self, id: VarId) -> String {
        let var = &self.vars[id.0 as usize];
        match var.parent_scope {
            Some(scope_id) => format!("{}.{}", self.scope_full_path(scope_id), var.name),
            None => var.name.clone(),
        }
    }

    /// Bit width of a variable.
    pub fn var_width(&self, id: VarId) -> u32 {
        self.vars[id.0 as usize].width
    }

    /// Type of a variable.
    pub fn var_type(&self, id: VarId) -> VarType {
        self.vars[id.0 as usize].var_type
    }

    /// Direction of a variable.
    pub fn var_direction(&self, id: VarId) -> VarDirection {
        self.vars[id.0 as usize].direction
    }

    /// Unique signal id backing this variable (multiple vars may alias one signal).
    pub fn var_signal_id(&self, id: VarId) -> SignalId {
        self.vars[id.0 as usize].signal_id
    }

    // -- Search --

    /// Find variables whose full path matches a glob pattern.
    pub fn find_vars(&self, pattern: &str) -> Vec<VarId> {
        let pat = match glob::Pattern::new(pattern) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        (0..self.vars.len())
            .map(|i| VarId(i as u32))
            .filter(|&id| pat.matches(&self.var_full_path(id)))
            .collect()
    }

    // -- Counts --

    /// Number of scopes in the hierarchy.
    pub fn scope_count(&self) -> usize {
        self.scopes.len()
    }

    /// Number of variables in the hierarchy.
    pub fn var_count(&self) -> usize {
        self.vars.len()
    }

    /// Number of unique signals (after alias resolution).
    pub fn signal_count(&self) -> usize {
        self.unique_signals.len()
    }
}

// ---------------------------------------------------------------------------
// HierarchyBuilder
// ---------------------------------------------------------------------------

/// Builds a `Hierarchy` from a stream of `HierarchyEvent`s.
pub struct HierarchyBuilder {
    scopes: Vec<ScopeData>,
    vars: Vec<VarData>,
    top_scopes: Vec<ScopeId>,
    scope_stack: Vec<ScopeId>,
    unique_signals: HashSet<SignalId>,
}

impl HierarchyBuilder {
    pub fn new() -> Self {
        Self {
            scopes: Vec::new(),
            vars: Vec::new(),
            top_scopes: Vec::new(),
            scope_stack: Vec::new(),
            unique_signals: HashSet::new(),
        }
    }

    /// Feed a single event into the builder.
    pub fn event(&mut self, event: HierarchyEvent) {
        match event {
            HierarchyEvent::EnterScope { name, scope_type } => {
                let id = ScopeId(self.scopes.len() as u32);
                let parent = self.scope_stack.last().copied();
                self.scopes.push(ScopeData {
                    name,
                    scope_type,
                    parent,
                    children: Vec::new(),
                    vars: Vec::new(),
                });
                if let Some(parent_id) = parent {
                    self.scopes[parent_id.0 as usize].children.push(id);
                } else {
                    self.top_scopes.push(id);
                }
                self.scope_stack.push(id);
            }
            HierarchyEvent::ExitScope => {
                self.scope_stack.pop();
            }
            HierarchyEvent::Var {
                name,
                var_type,
                direction,
                width,
                signal_id,
                is_alias: _,
            } => {
                let id = VarId(self.vars.len() as u32);
                let parent_scope = self.scope_stack.last().copied();
                self.vars.push(VarData {
                    name,
                    var_type,
                    direction,
                    width,
                    signal_id,
                    parent_scope,
                });
                if let Some(scope_id) = parent_scope {
                    self.scopes[scope_id.0 as usize].vars.push(id);
                }
                self.unique_signals.insert(signal_id);
            }
        }
    }

    /// Consume the builder and produce the finished `Hierarchy`.
    pub fn build(self) -> Hierarchy {
        Hierarchy {
            scopes: self.scopes,
            vars: self.vars,
            top_scopes: self.top_scopes,
            unique_signals: self.unique_signals,
        }
    }
}

impl Default for HierarchyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    /// Helper: build a small hierarchy with 2 scopes, 3 vars.
    ///
    /// ```text
    /// top (Module)
    /// ├── clk        [1-bit, Wire, signal 0]
    /// ├── reset      [1-bit, Wire, signal 1]
    /// └── sub (Generate)
    ///     └── data   [8-bit, Reg, signal 2]
    /// ```
    fn build_small_hierarchy() -> Hierarchy {
        let mut b = HierarchyBuilder::new();
        b.event(HierarchyEvent::EnterScope {
            name: "top".into(),
            scope_type: ScopeType::Module,
        });
        b.event(HierarchyEvent::Var {
            name: "clk".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Input,
            width: 1,
            signal_id: SignalId(0),
            is_alias: false,
        });
        b.event(HierarchyEvent::Var {
            name: "reset".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Input,
            width: 1,
            signal_id: SignalId(1),
            is_alias: false,
        });
        b.event(HierarchyEvent::EnterScope {
            name: "sub".into(),
            scope_type: ScopeType::Generate,
        });
        b.event(HierarchyEvent::Var {
            name: "data".into(),
            var_type: VarType::Reg,
            direction: VarDirection::Implicit,
            width: 8,
            signal_id: SignalId(2),
            is_alias: false,
        });
        b.event(HierarchyEvent::ExitScope); // sub
        b.event(HierarchyEvent::ExitScope); // top
        b.build()
    }

    #[test]
    fn top_scopes() {
        let h = build_small_hierarchy();
        assert_eq!(h.top_scopes().len(), 1);
        assert_eq!(h.scope_name(h.top_scopes()[0]), "top");
    }

    #[test]
    fn scope_children() {
        let h = build_small_hierarchy();
        let top = h.top_scopes()[0];
        let children = h.scope_children(top);
        assert_eq!(children.len(), 1);
        assert_eq!(h.scope_name(children[0]), "sub");
    }

    #[test]
    fn scope_vars() {
        let h = build_small_hierarchy();
        let top = h.top_scopes()[0];
        let vars = h.scope_vars(top);
        assert_eq!(vars.len(), 2);
        assert_eq!(h.var_name(vars[0]), "clk");
        assert_eq!(h.var_name(vars[1]), "reset");
    }

    #[test]
    fn scope_parent() {
        let h = build_small_hierarchy();
        let top = h.top_scopes()[0];
        assert_eq!(h.scope_parent(top), None);
        let sub = h.scope_children(top)[0];
        assert_eq!(h.scope_parent(sub), Some(top));
    }

    #[test]
    fn scope_name_and_type() {
        let h = build_small_hierarchy();
        let top = h.top_scopes()[0];
        assert_eq!(h.scope_name(top), "top");
        assert_eq!(h.scope_type(top), ScopeType::Module);
        let sub = h.scope_children(top)[0];
        assert_eq!(h.scope_name(sub), "sub");
        assert_eq!(h.scope_type(sub), ScopeType::Generate);
    }

    #[test]
    fn scope_full_path() {
        let h = build_small_hierarchy();
        let top = h.top_scopes()[0];
        assert_eq!(h.scope_full_path(top), "top");
        let sub = h.scope_children(top)[0];
        assert_eq!(h.scope_full_path(sub), "top.sub");
    }

    #[test]
    fn var_metadata() {
        let h = build_small_hierarchy();
        let top = h.top_scopes()[0];
        let clk = h.scope_vars(top)[0];
        assert_eq!(h.var_name(clk), "clk");
        assert_eq!(h.var_width(clk), 1);
        assert_eq!(h.var_type(clk), VarType::Wire);
        assert_eq!(h.var_direction(clk), VarDirection::Input);
        assert_eq!(h.var_signal_id(clk), SignalId(0));
    }

    #[test]
    fn var_full_path() {
        let h = build_small_hierarchy();
        let top = h.top_scopes()[0];
        let clk = h.scope_vars(top)[0];
        assert_eq!(h.var_full_path(clk), "top.clk");

        let sub = h.scope_children(top)[0];
        let data = h.scope_vars(sub)[0];
        assert_eq!(h.var_full_path(data), "top.sub.data");
    }

    #[test]
    fn find_vars_glob() {
        let h = build_small_hierarchy();
        let results = h.find_vars("*.clk");
        assert_eq!(results.len(), 1);
        assert_eq!(h.var_name(results[0]), "clk");

        let results = h.find_vars("top.*");
        assert_eq!(results.len(), 3); // clk, reset, sub.data — glob * matches dots

        let results = h.find_vars("**");
        assert_eq!(results.len(), 3); // all vars
    }

    #[test]
    fn counts() {
        let h = build_small_hierarchy();
        assert_eq!(h.scope_count(), 2);
        assert_eq!(h.var_count(), 3);
        assert_eq!(h.signal_count(), 3);
    }

    #[test]
    fn deep_nesting() {
        // 4 levels: a > b > c > d with a var at the leaf
        let mut b = HierarchyBuilder::new();
        for name in &["a", "b", "c", "d"] {
            b.event(HierarchyEvent::EnterScope {
                name: name.to_string(),
                scope_type: ScopeType::Module,
            });
        }
        b.event(HierarchyEvent::Var {
            name: "sig".into(),
            var_type: VarType::Logic,
            direction: VarDirection::Implicit,
            width: 4,
            signal_id: SignalId(0),
            is_alias: false,
        });
        for _ in 0..4 {
            b.event(HierarchyEvent::ExitScope);
        }
        let h = b.build();

        assert_eq!(h.scope_count(), 4);
        assert_eq!(h.var_count(), 1);
        let sig = VarId(0);
        assert_eq!(h.var_full_path(sig), "a.b.c.d.sig");
    }

    #[test]
    fn alias_same_signal_id() {
        // Two vars with the same SignalId — signal_count should be 1
        let mut b = HierarchyBuilder::new();
        b.event(HierarchyEvent::EnterScope {
            name: "top".into(),
            scope_type: ScopeType::Module,
        });
        b.event(HierarchyEvent::Var {
            name: "a".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: SignalId(42),
            is_alias: false,
        });
        b.event(HierarchyEvent::Var {
            name: "b".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: SignalId(42),
            is_alias: true,
        });
        b.event(HierarchyEvent::ExitScope);
        let h = b.build();

        assert_eq!(h.var_count(), 2);
        assert_eq!(h.signal_count(), 1);
        assert_eq!(h.var_signal_id(VarId(0)), h.var_signal_id(VarId(1)));
    }

    #[test]
    fn empty_hierarchy() {
        let h = HierarchyBuilder::new().build();
        assert_eq!(h.scope_count(), 0);
        assert_eq!(h.var_count(), 0);
        assert_eq!(h.signal_count(), 0);
        assert!(h.top_scopes().is_empty());
    }

    #[test]
    fn find_vars_invalid_pattern() {
        let h = build_small_hierarchy();
        // Invalid glob pattern should return empty
        let results = h.find_vars("[invalid");
        assert!(results.is_empty());
    }
}
