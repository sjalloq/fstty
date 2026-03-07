//! Signal selection and filtering

use std::collections::HashSet;

use wellen::{Hierarchy, ScopeRef, SignalRef, VarRef};

use crate::error::{Error, Result};
use crate::hierarchy_legacy::HierarchyNavigator;

/// Represents a filter pattern for signals
#[derive(Debug, Clone)]
pub enum FilterPattern {
    /// Glob pattern (e.g., "cpu.*.clk")
    Glob(glob::Pattern),
    /// Regex pattern
    Regex(regex::Regex),
    /// Match all
    All,
}

impl FilterPattern {
    /// Create a glob pattern
    pub fn glob(pattern: &str) -> Result<Self> {
        let pattern =
            glob::Pattern::new(pattern).map_err(|e| Error::InvalidPattern(e.to_string()))?;
        Ok(FilterPattern::Glob(pattern))
    }

    /// Create a regex pattern
    pub fn regex(pattern: &str) -> Result<Self> {
        let regex = regex::Regex::new(pattern).map_err(|e| Error::InvalidPattern(e.to_string()))?;
        Ok(FilterPattern::Regex(regex))
    }

    /// Check if a path matches this pattern
    pub fn matches(&self, path: &str) -> bool {
        match self {
            FilterPattern::Glob(pattern) => pattern.matches(path),
            FilterPattern::Regex(regex) => regex.is_match(path),
            FilterPattern::All => true,
        }
    }
}

/// Selection state for signals
/// Note: VarRef doesn't implement Hash, so we store indices instead
#[derive(Debug, Default)]
pub struct SignalSelection {
    /// Explicitly selected signal indices (VarRef.index())
    selected_signal_indices: HashSet<usize>,
    /// Selected scope subtrees (all signals within)
    selected_scopes: HashSet<ScopeRef>,
    /// Active filter pattern
    filter: Option<FilterPattern>,
}

impl SignalSelection {
    /// Create a new empty selection
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the filter pattern
    pub fn set_filter(&mut self, pattern: Option<FilterPattern>) {
        self.filter = pattern;
    }

    /// Get the current filter pattern
    pub fn filter(&self) -> Option<&FilterPattern> {
        self.filter.as_ref()
    }

    /// Toggle selection of a signal
    pub fn toggle_signal(&mut self, var_ref: VarRef) {
        let index = var_ref.index();
        if self.selected_signal_indices.contains(&index) {
            self.selected_signal_indices.remove(&index);
        } else {
            self.selected_signal_indices.insert(index);
        }
    }

    /// Select a signal
    pub fn select_signal(&mut self, var_ref: VarRef) {
        self.selected_signal_indices.insert(var_ref.index());
    }

    /// Deselect a signal
    pub fn deselect_signal(&mut self, var_ref: VarRef) {
        self.selected_signal_indices.remove(&var_ref.index());
    }

    /// Check if a signal is selected
    pub fn is_signal_selected(&self, var_ref: VarRef) -> bool {
        self.selected_signal_indices.contains(&var_ref.index())
    }

    /// Toggle selection of a scope (and all signals within)
    pub fn toggle_scope(&mut self, scope_ref: ScopeRef) {
        if self.selected_scopes.contains(&scope_ref) {
            self.selected_scopes.remove(&scope_ref);
        } else {
            self.selected_scopes.insert(scope_ref);
        }
    }

    /// Select a scope
    pub fn select_scope(&mut self, scope_ref: ScopeRef) {
        self.selected_scopes.insert(scope_ref);
    }

    /// Deselect a scope
    pub fn deselect_scope(&mut self, scope_ref: ScopeRef) {
        self.selected_scopes.remove(&scope_ref);
    }

    /// Check if a scope is selected
    pub fn is_scope_selected(&self, scope_ref: ScopeRef) -> bool {
        self.selected_scopes.contains(&scope_ref)
    }

    /// Clear all selections
    pub fn clear(&mut self) {
        self.selected_signal_indices.clear();
        self.selected_scopes.clear();
    }

    /// Check if anything is selected
    pub fn is_empty(&self) -> bool {
        self.selected_signal_indices.is_empty() && self.selected_scopes.is_empty()
    }

    /// Get count of explicitly selected signals
    pub fn selected_signal_count(&self) -> usize {
        self.selected_signal_indices.len()
    }

    /// Get count of selected scopes
    pub fn selected_scope_count(&self) -> usize {
        self.selected_scopes.len()
    }

    /// Resolve selection to a list of all matching SignalRefs
    /// This applies both explicit selection and filter patterns
    pub fn resolve<'a>(&'a self, hierarchy: &'a Hierarchy) -> impl Iterator<Item = SignalRef> + 'a {
        let nav = HierarchyNavigator::new(hierarchy);

        // Enumerate variables with their VarRefs
        hierarchy
            .vars()
            .filter_map(move |var_ref| {
                let var = &hierarchy[var_ref];

                // If there's an explicit selection, check it
                if !self.is_empty() {
                    // Check explicit signal selection
                    if !self.selected_signal_indices.contains(&var_ref.index()) {
                        // Check if in a selected scope
                        let var_path = var.full_name(hierarchy);
                        let in_selected_scope = self.selected_scopes.iter().any(|&scope_ref| {
                            let scope_path = hierarchy[scope_ref].full_name(hierarchy);
                            var_path.starts_with(&scope_path)
                        });
                        if !in_selected_scope {
                            return None;
                        }
                    }
                }

                // If there's a filter, check it
                if let Some(ref filter) = self.filter {
                    let path = nav.var_path_string(var_ref);
                    if !filter.matches(&path) {
                        return None;
                    }
                }

                Some(var.signal_ref())
            })
    }

    /// Estimate the count of matching signals (may iterate all signals)
    pub fn estimate_count(&self, hierarchy: &Hierarchy) -> usize {
        self.resolve(hierarchy).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_pattern() {
        let pattern = FilterPattern::glob("*.clk").unwrap();
        assert!(pattern.matches("cpu.clk"));
        assert!(pattern.matches("memory.clk"));
        assert!(!pattern.matches("cpu.data"));
    }

    #[test]
    fn test_regex_pattern() {
        let pattern = FilterPattern::regex(r"cpu\.\w+\.clk").unwrap();
        assert!(pattern.matches("cpu.core0.clk"));
        assert!(pattern.matches("cpu.cache.clk"));
        assert!(!pattern.matches("memory.clk"));
    }
}
