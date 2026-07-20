//! Bytecode-owned slot metadata serialized with compiled functions.

use serde::{Deserialize, Serialize};

/// Tag identifying which typed local storage a variable is stored in.
///
/// The compiler serializes this as static slot metadata on `FunctionInfo` and
/// `CompiledProgram`; the VM also uses it at runtime to select the matching
/// frame storage path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VarTypeTag {
    I64,
    F64,
    F32,
    F16,
    Str,
    Char,
    Array,
    Tuple,
    NamedTuple,
    Dict,
    Set,
    Struct,
    Range,
    Rng,
    Generator,
    Any,
    NarrowInt,
    Nothing,
    Bool,
    ValSymbol,
    Symbol,
}
