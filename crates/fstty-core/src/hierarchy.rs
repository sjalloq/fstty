//! Hierarchy navigation helpers

use std::collections::HashSet;

use wellen::{Hierarchy, ScopeRef, Var, VarRef};

/// Represents a node in the hierarchy tree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyNode {
    Scope(ScopeRef),
    Variable(VarRef),
}

/// Helper for navigating and querying the waveform hierarchy
pub struct HierarchyNavigator<'a> {
    hierarchy: &'a Hierarchy,
}

impl<'a> HierarchyNavigator<'a> {
    /// Create a new navigator for the given hierarchy
    pub fn new(hierarchy: &'a Hierarchy) -> Self {
        Self { hierarchy }
    }

    /// Get the underlying hierarchy
    pub fn hierarchy(&self) -> &Hierarchy {
        self.hierarchy
    }

    /// Get the full path for a scope as a dot-separated string
    pub fn scope_path_string(&self, scope_ref: ScopeRef) -> String {
        self.hierarchy[scope_ref].full_name(self.hierarchy)
    }

    /// Get the full path for a variable as a dot-separated string
    pub fn var_path_string(&self, var_ref: VarRef) -> String {
        self.hierarchy[var_ref].full_name(self.hierarchy)
    }

    /// Count descendants of a scope (scopes, vars)
    pub fn count_descendants(&self, scope_ref: ScopeRef) -> (usize, usize) {
        let mut scope_count = 0;
        let mut var_count = 0;

        self.visit_descendants(scope_ref, &mut |node| {
            match node {
                HierarchyNode::Scope(_) => scope_count += 1,
                HierarchyNode::Variable(_) => var_count += 1,
            }
            true // continue visiting
        });

        (scope_count, var_count)
    }

    /// Visit all descendants of a scope
    fn visit_descendants<F>(&self, scope_ref: ScopeRef, visitor: &mut F)
    where
        F: FnMut(HierarchyNode) -> bool,
    {
        let scope = &self.hierarchy[scope_ref];

        // Visit child scopes
        for child_scope in scope.scopes(self.hierarchy) {
            if !visitor(HierarchyNode::Scope(child_scope)) {
                return;
            }
            self.visit_descendants(child_scope, visitor);
        }

        // Visit child variables
        for child_var in scope.vars(self.hierarchy) {
            if !visitor(HierarchyNode::Variable(child_var)) {
                return;
            }
        }
    }

    /// Get top-level scopes
    pub fn top_scopes(&self) -> impl Iterator<Item = ScopeRef> + '_ {
        self.hierarchy.scopes()
    }

    /// Get top-level variables (rare, but possible)
    pub fn top_vars(&self) -> impl Iterator<Item = VarRef> + '_ {
        self.hierarchy.vars()
    }

    /// Get child scopes of a scope
    pub fn child_scopes(&self, scope_ref: ScopeRef) -> impl Iterator<Item = ScopeRef> + '_ {
        self.hierarchy[scope_ref].scopes(self.hierarchy)
    }

    /// Get child variables of a scope
    pub fn child_vars(&self, scope_ref: ScopeRef) -> impl Iterator<Item = VarRef> + '_ {
        self.hierarchy[scope_ref].vars(self.hierarchy)
    }

    /// Check if a scope has any children (scopes or vars)
    pub fn has_children(&self, scope_ref: ScopeRef) -> bool {
        let scope = &self.hierarchy[scope_ref];
        scope.scopes(self.hierarchy).next().is_some()
            || scope.vars(self.hierarchy).next().is_some()
    }

    /// Get variable by reference
    pub fn get_var(&self, var_ref: VarRef) -> &Var {
        &self.hierarchy[var_ref]
    }

    /// Iterate visible nodes given expansion state
    /// Returns (node, depth) pairs for rendering
    pub fn visible_nodes<'b>(
        &'a self,
        expanded: &'b HashSet<ScopeRef>,
    ) -> impl Iterator<Item = (HierarchyNode, usize)> + 'b
    where
        'a: 'b,
    {
        VisibleNodeIterator::new(self.hierarchy, expanded)
    }
}

/// Iterator over visible nodes in the hierarchy
pub struct VisibleNodeIterator<'a> {
    hierarchy: &'a Hierarchy,
    expanded: &'a HashSet<ScopeRef>,
    stack: Vec<(HierarchyNode, usize)>,
}

impl<'a> VisibleNodeIterator<'a> {
    fn new(hierarchy: &'a Hierarchy, expanded: &'a HashSet<ScopeRef>) -> Self {
        let mut stack = Vec::new();

        // Add top-level items in reverse order (so first item is popped first)
        let top_vars: Vec<_> = hierarchy.vars().collect();
        for var_ref in top_vars.into_iter().rev() {
            stack.push((HierarchyNode::Variable(var_ref), 0));
        }

        let top_scopes: Vec<_> = hierarchy.scopes().collect();
        for scope_ref in top_scopes.into_iter().rev() {
            stack.push((HierarchyNode::Scope(scope_ref), 0));
        }

        Self {
            hierarchy,
            expanded,
            stack,
        }
    }
}

impl<'a> Iterator for VisibleNodeIterator<'a> {
    type Item = (HierarchyNode, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let (node, depth) = self.stack.pop()?;

        // If this is an expanded scope, add its children
        if let HierarchyNode::Scope(scope_ref) = node {
            if self.expanded.contains(&scope_ref) {
                let scope = &self.hierarchy[scope_ref];

                // Add children in reverse order
                let child_vars: Vec<_> = scope.vars(self.hierarchy).collect();
                for var_ref in child_vars.into_iter().rev() {
                    self.stack.push((HierarchyNode::Variable(var_ref), depth + 1));
                }

                let child_scopes: Vec<_> = scope.scopes(self.hierarchy).collect();
                for child_ref in child_scopes.into_iter().rev() {
                    self.stack.push((HierarchyNode::Scope(child_ref), depth + 1));
                }
            }
        }

        Some((node, depth))
    }
}

#[cfg(test)]
mod tests {
    // Tests would require a sample waveform file
}
