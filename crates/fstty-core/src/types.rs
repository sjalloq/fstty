//! Core types for fstty — IDs, enums, signal values, metadata.
//!
//! These types are fstty's own — no backend (wellen, fst-reader) types leak through.

use std::fmt;

// ---------------------------------------------------------------------------
// Opaque IDs
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub(crate) u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarId(pub(crate) u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignalId(pub(crate) u32);

impl fmt::Debug for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScopeId({})", self.0)
    }
}

impl fmt::Debug for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VarId({})", self.0)
    }
}

impl fmt::Debug for SignalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignalId({})", self.0)
    }
}

// ---------------------------------------------------------------------------
// Enum types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeType {
    Module,
    Task,
    Function,
    Begin,
    Fork,
    Generate,
    Struct,
    Union,
    Class,
    Interface,
    Package,
    Program,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarType {
    Wire,
    Reg,
    Logic,
    Integer,
    Real,
    Parameter,
    Event,
    Supply0,
    Supply1,
    Tri,
    TriAnd,
    TriOr,
    TriReg,
    Tri0,
    Tri1,
    WAnd,
    WOr,
    Port,
    SparseArray,
    RealTime,
    String,
    Bit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarDirection {
    Implicit,
    Input,
    Output,
    InOut,
    Buffer,
    Linkage,
}

// ---------------------------------------------------------------------------
// Signal value
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SignalValue<'a> {
    /// Bit-string, one ASCII char per bit ('0','1','x','z').
    Binary(&'a [u8]),
    Real(f64),
}

// ---------------------------------------------------------------------------
// Waveform metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct WaveformMetadata {
    /// Timescale = 10^exponent seconds (e.g. -9 for nanoseconds).
    pub timescale_exponent: i8,
    pub start_time: u64,
    pub end_time: u64,
    pub var_count: u64,
    pub signal_count: usize,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn id_copy_and_compare() {
        let a = ScopeId(1);
        let b = a; // Copy
        assert_eq!(a, b);

        let v1 = VarId(10);
        let v2 = VarId(10);
        let v3 = VarId(20);
        assert_eq!(v1, v2);
        assert_ne!(v1, v3);

        let s1 = SignalId(5);
        let s2 = SignalId(5);
        assert_eq!(s1, s2);
    }

    #[test]
    fn id_as_hashmap_key() {
        let mut scope_map: HashMap<ScopeId, &str> = HashMap::new();
        scope_map.insert(ScopeId(0), "top");
        scope_map.insert(ScopeId(1), "sub");
        assert_eq!(scope_map[&ScopeId(0)], "top");
        assert_eq!(scope_map[&ScopeId(1)], "sub");

        let mut var_map: HashMap<VarId, u32> = HashMap::new();
        var_map.insert(VarId(0), 8);
        assert_eq!(var_map[&VarId(0)], 8);

        let mut sig_map: HashMap<SignalId, &str> = HashMap::new();
        sig_map.insert(SignalId(0), "clk");
        assert_eq!(sig_map[&SignalId(0)], "clk");
    }

    #[test]
    fn id_debug_format() {
        assert_eq!(format!("{:?}", ScopeId(42)), "ScopeId(42)");
        assert_eq!(format!("{:?}", VarId(7)), "VarId(7)");
        assert_eq!(format!("{:?}", SignalId(99)), "SignalId(99)");
    }

    #[test]
    fn scope_type_variants() {
        let variants = [
            ScopeType::Module,
            ScopeType::Task,
            ScopeType::Function,
            ScopeType::Begin,
            ScopeType::Fork,
            ScopeType::Generate,
            ScopeType::Struct,
            ScopeType::Union,
            ScopeType::Class,
            ScopeType::Interface,
            ScopeType::Package,
            ScopeType::Program,
        ];
        for v in &variants {
            let s = format!("{:?}", v);
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn var_type_variants() {
        let variants = [
            VarType::Wire,
            VarType::Reg,
            VarType::Logic,
            VarType::Integer,
            VarType::Real,
            VarType::Parameter,
            VarType::Event,
            VarType::Supply0,
            VarType::Supply1,
            VarType::Tri,
            VarType::TriAnd,
            VarType::TriOr,
            VarType::TriReg,
            VarType::Tri0,
            VarType::Tri1,
            VarType::WAnd,
            VarType::WOr,
            VarType::Port,
            VarType::SparseArray,
            VarType::RealTime,
            VarType::String,
            VarType::Bit,
        ];
        for v in &variants {
            let s = format!("{:?}", v);
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn var_direction_variants() {
        let variants = [
            VarDirection::Implicit,
            VarDirection::Input,
            VarDirection::Output,
            VarDirection::InOut,
            VarDirection::Buffer,
            VarDirection::Linkage,
        ];
        for v in &variants {
            let s = format!("{:?}", v);
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn signal_value_binary() {
        let bits: &[u8] = b"01xz";
        let val = SignalValue::Binary(bits);
        match val {
            SignalValue::Binary(b) => assert_eq!(b, b"01xz"),
            _ => panic!("expected Binary"),
        }
    }

    #[test]
    fn signal_value_real() {
        let val = SignalValue::Real(3.14);
        match val {
            SignalValue::Real(r) => assert!((r - 3.14).abs() < f64::EPSILON),
            _ => panic!("expected Real"),
        }
    }

    #[test]
    fn waveform_metadata_construct() {
        let meta = WaveformMetadata {
            timescale_exponent: -9,
            start_time: 0,
            end_time: 1_000_000,
            var_count: 128,
            signal_count: 100,
        };
        assert_eq!(meta.timescale_exponent, -9);
        assert_eq!(meta.start_time, 0);
        assert_eq!(meta.end_time, 1_000_000);
        assert_eq!(meta.var_count, 128);
        assert_eq!(meta.signal_count, 100);
    }
}
