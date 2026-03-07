//! Wellen hierarchy adapter.
//!
//! Walks wellen's parsed hierarchy arena and emits [`HierarchyEvent`]s,
//! which are fed into [`HierarchyBuilder`] to produce fstty's own [`Hierarchy`].

use crate::hierarchy::{Hierarchy, HierarchyBuilder, HierarchyEvent};
use crate::types::{ScopeType, SignalId, VarDirection, VarType};

/// Build an fstty `Hierarchy` from a wellen `Hierarchy`.
pub fn build_hierarchy_from_wellen(wh: &wellen::Hierarchy) -> Hierarchy {
    let mut builder = HierarchyBuilder::new();
    // Walk top-level scopes
    for scope_ref in wh.scopes() {
        walk_scope(wh, scope_ref, &mut builder);
    }
    // Walk top-level vars (rare, but possible)
    for var_ref in wh.vars() {
        emit_var(wh, var_ref, &mut builder);
    }
    builder.build()
}

fn walk_scope(
    wh: &wellen::Hierarchy,
    scope_ref: wellen::ScopeRef,
    builder: &mut HierarchyBuilder,
) {
    let scope = &wh[scope_ref];
    builder.event(HierarchyEvent::EnterScope {
        name: scope.name(wh).to_string(),
        scope_type: convert_scope_type(scope.scope_type()),
    });

    // Iterate children in declaration order (scopes and vars interleaved)
    for item in scope.items(wh) {
        match item {
            wellen::ScopeOrVarRef::Scope(child_ref) => {
                walk_scope(wh, child_ref, builder);
            }
            wellen::ScopeOrVarRef::Var(var_ref) => {
                emit_var(wh, var_ref, builder);
            }
        }
    }

    builder.event(HierarchyEvent::ExitScope);
}

fn emit_var(wh: &wellen::Hierarchy, var_ref: wellen::VarRef, builder: &mut HierarchyBuilder) {
    let var = &wh[var_ref];
    let width = match var.signal_encoding() {
        wellen::SignalEncoding::BitVector(n) => n.get(),
        wellen::SignalEncoding::Real => 64,
        wellen::SignalEncoding::String => 0,
    };

    builder.event(HierarchyEvent::Var {
        name: var.name(wh).to_string(),
        var_type: convert_var_type(var.var_type()),
        direction: convert_var_direction(var.direction()),
        width,
        signal_id: SignalId(var.signal_ref().index() as u32),
        is_alias: false, // wellen doesn't surface alias info directly; HierarchyBuilder dedupes via SignalId
    });
}

fn convert_scope_type(st: wellen::ScopeType) -> ScopeType {
    match st {
        wellen::ScopeType::Module => ScopeType::Module,
        wellen::ScopeType::Task => ScopeType::Task,
        wellen::ScopeType::Function => ScopeType::Function,
        wellen::ScopeType::Begin => ScopeType::Begin,
        wellen::ScopeType::Fork => ScopeType::Fork,
        wellen::ScopeType::Generate => ScopeType::Generate,
        wellen::ScopeType::Struct => ScopeType::Struct,
        wellen::ScopeType::Union => ScopeType::Union,
        wellen::ScopeType::Class => ScopeType::Class,
        wellen::ScopeType::Interface => ScopeType::Interface,
        wellen::ScopeType::Package => ScopeType::Package,
        wellen::ScopeType::Program => ScopeType::Program,
        // VHDL and other scope types map to Module as a reasonable default
        _ => ScopeType::Module,
    }
}

fn convert_var_type(vt: wellen::VarType) -> VarType {
    match vt {
        wellen::VarType::Wire => VarType::Wire,
        wellen::VarType::Reg => VarType::Reg,
        wellen::VarType::Logic => VarType::Logic,
        wellen::VarType::Integer | wellen::VarType::Int => VarType::Integer,
        wellen::VarType::Real | wellen::VarType::ShortReal => VarType::Real,
        wellen::VarType::Parameter => VarType::Parameter,
        wellen::VarType::Event => VarType::Event,
        wellen::VarType::Supply0 => VarType::Supply0,
        wellen::VarType::Supply1 => VarType::Supply1,
        wellen::VarType::Tri => VarType::Tri,
        wellen::VarType::TriAnd => VarType::TriAnd,
        wellen::VarType::TriOr => VarType::TriOr,
        wellen::VarType::TriReg => VarType::TriReg,
        wellen::VarType::Tri0 => VarType::Tri0,
        wellen::VarType::Tri1 => VarType::Tri1,
        wellen::VarType::WAnd => VarType::WAnd,
        wellen::VarType::WOr => VarType::WOr,
        wellen::VarType::Port => VarType::Port,
        wellen::VarType::SparseArray => VarType::SparseArray,
        wellen::VarType::RealTime | wellen::VarType::Time => VarType::RealTime,
        wellen::VarType::String => VarType::String,
        wellen::VarType::Bit => VarType::Bit,
        // VHDL types map to closest equivalent
        wellen::VarType::Boolean => VarType::Logic,
        wellen::VarType::BitVector | wellen::VarType::StdLogicVector | wellen::VarType::StdULogicVector => VarType::Logic,
        wellen::VarType::StdLogic | wellen::VarType::StdULogic => VarType::Logic,
        wellen::VarType::Enum => VarType::Integer,
        wellen::VarType::ShortInt => VarType::Integer,
        wellen::VarType::LongInt => VarType::Integer,
        wellen::VarType::Byte => VarType::Integer,
    }
}

fn convert_var_direction(vd: wellen::VarDirection) -> VarDirection {
    match vd {
        wellen::VarDirection::Implicit => VarDirection::Implicit,
        wellen::VarDirection::Input => VarDirection::Input,
        wellen::VarDirection::Output => VarDirection::Output,
        wellen::VarDirection::InOut => VarDirection::InOut,
        wellen::VarDirection::Buffer => VarDirection::Buffer,
        wellen::VarDirection::Linkage => VarDirection::Linkage,
        // wellen has Unknown which maps to Implicit for us
        _ => VarDirection::Implicit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use wellen::{LoadOptions, viewers};

    /// Small test FST file for fast tests.
    const TEST_FST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rv32_soc_TB.vcd.fst");

    fn load_wellen_hierarchy(path: &str) -> wellen::Hierarchy {
        let opts = LoadOptions::default();
        let header =
            viewers::read_header_from_file(Path::new(path), &opts).expect("failed to open FST");
        header.hierarchy
    }

    #[test]
    fn scope_count_nonzero() {
        let wh = load_wellen_hierarchy(TEST_FST);
        let h = build_hierarchy_from_wellen(&wh);
        assert!(h.scope_count() > 0, "expected at least one scope");
    }

    #[test]
    fn var_count_nonzero() {
        let wh = load_wellen_hierarchy(TEST_FST);
        let h = build_hierarchy_from_wellen(&wh);
        assert!(h.var_count() > 0, "expected at least one var");
    }

    #[test]
    fn top_scope_name() {
        let wh = load_wellen_hierarchy(TEST_FST);
        let h = build_hierarchy_from_wellen(&wh);
        let top = h.top_scopes();
        assert!(!top.is_empty(), "expected top-level scopes");
        // The first top-level scope should have a non-empty name
        let name = h.scope_name(top[0]);
        assert!(!name.is_empty(), "top scope should have a name");
    }

    #[test]
    fn scope_type_is_module() {
        let wh = load_wellen_hierarchy(TEST_FST);
        let h = build_hierarchy_from_wellen(&wh);
        let top = h.top_scopes()[0];
        // Top-level scopes in Verilog are typically Module
        assert_eq!(h.scope_type(top), ScopeType::Module);
    }

    #[test]
    fn var_full_path_is_dotted() {
        let wh = load_wellen_hierarchy(TEST_FST);
        let h = build_hierarchy_from_wellen(&wh);
        // Find a var that's nested (has a dot in its path)
        let has_dotted = (0..h.var_count())
            .map(|i| crate::types::VarId(i as u32))
            .any(|vid| h.var_full_path(vid).contains('.'));
        assert!(has_dotted, "expected at least one var with dotted path");
    }

    #[test]
    fn signal_count_matches_wellen() {
        let wh = load_wellen_hierarchy(TEST_FST);
        let wellen_unique = wh.num_unique_signals();
        let h = build_hierarchy_from_wellen(&wh);
        assert_eq!(
            h.signal_count(),
            wellen_unique,
            "signal_count should match wellen's num_unique_signals"
        );
    }

    #[test]
    fn scope_full_path_matches_wellen() {
        let wh = load_wellen_hierarchy(TEST_FST);
        let h = build_hierarchy_from_wellen(&wh);

        // Compare full paths of first few scopes between wellen and our hierarchy
        let mut wellen_scope_names = Vec::new();
        for scope_ref in wh.scopes() {
            wellen_scope_names.push(wh[scope_ref].full_name(&wh));
        }

        for (i, expected_path) in wellen_scope_names.iter().enumerate() {
            let our_path = h.scope_full_path(h.top_scopes()[i]);
            assert_eq!(&our_path, expected_path, "scope path mismatch at index {i}");
        }
    }
}
