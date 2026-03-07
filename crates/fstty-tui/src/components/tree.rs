//! Hierarchy tree component

use std::collections::HashSet;

use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem};
use wellen::ScopeRef;

use fstty_core::hierarchy_legacy::{HierarchyNavigator, HierarchyNode};

/// Tree component for displaying waveform hierarchy
pub struct HierarchyTree<'a> {
    navigator: &'a HierarchyNavigator<'a>,
    expanded: &'a HashSet<ScopeRef>,
    cursor: usize,
}

impl<'a> HierarchyTree<'a> {
    pub fn new(
        navigator: &'a HierarchyNavigator<'a>,
        expanded: &'a HashSet<ScopeRef>,
        cursor: usize,
    ) -> Self {
        Self {
            navigator,
            expanded,
            cursor,
        }
    }

    /// Build the tree widget, returns (widget, visible_count)
    pub fn build(&self) -> (List<'a>, usize) {
        let hierarchy = self.navigator.hierarchy();
        let mut items = Vec::new();

        for (node, depth) in self.navigator.visible_nodes(self.expanded) {
            let indent = "  ".repeat(depth);
            let (prefix, name) = match node {
                HierarchyNode::Scope(scope_ref) => {
                    let scope = &hierarchy[scope_ref];
                    let name = scope.name(hierarchy);
                    let has_children = self.navigator.has_children(scope_ref);
                    let prefix = if has_children {
                        if self.expanded.contains(&scope_ref) {
                            "▼ "
                        } else {
                            "▶ "
                        }
                    } else {
                        "  "
                    };
                    (prefix, name.to_string())
                }
                HierarchyNode::Variable(var_ref) => {
                    let var = &hierarchy[var_ref];
                    let name = var.name(hierarchy);
                    let width = var.length().unwrap_or(1);
                    let name_with_width = if width > 1 {
                        format!("{}[{}:0]", name, width - 1)
                    } else {
                        name.to_string()
                    };
                    ("• ", name_with_width)
                }
            };

            items.push(ListItem::new(format!("{}{}{}", indent, prefix, name)));
        }

        let count = items.len();

        let list = List::new(items)
            .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
            .highlight_symbol("› ");

        (list, count)
    }
}
