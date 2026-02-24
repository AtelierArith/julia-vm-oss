//! Equality builtin functions for the VM.
//!
//! Object identity and equality: ===, isequal, hash.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_utils::{normalize_struct_name, normalize_type_for_isa};
use super::value::Value;
use super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute equality builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not an equality builtin.
    pub(super) fn execute_builtin_equality(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::Egal => {
                // === (object identity)
                // For primitives: value equality
                // For reference types (Array, Dict, mutable struct): reference identity
                // NaN === NaN: false (IEEE 754 compliant)
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                let is_identical = match (&left, &right) {
                    // Primitives: value equality
                    (Value::I64(a), Value::I64(b)) => a == b,
                    (Value::F64(a), Value::F64(b)) => {
                        // === checks bit identity: NaN === NaN is true, -0.0 === 0.0 is false
                        a.to_bits() == b.to_bits()
                    }
                    (Value::Str(a), Value::Str(b)) => a == b,
                    (Value::Char(a), Value::Char(b)) => a == b,
                    (Value::Nothing, Value::Nothing) => true,
                    (Value::Missing, Value::Missing) => true,

                    // Symbols: name equality (symbols are interned)
                    (Value::Symbol(a), Value::Symbol(b)) => a == b,

                    // Reference types: check if same reference (by index/pointer)
                    // Arrays: same reference = same object
                    (Value::Array(a), Value::Array(b)) => std::ptr::eq(a.as_ptr(), b.as_ptr()),
                    // Memory → Array (Issue #2764)
                    (Value::Memory(a), Value::Memory(b)) => std::ptr::eq(a.as_ptr(), b.as_ptr()),

                    // Mutable structs: same reference = same object
                    (Value::StructRef(a), Value::StructRef(b)) => a == b,

                    // Immutable structs: structural equality (all fields ===)
                    // For simplicity, compare struct_name and all values by Debug representation
                    // NOTE: We normalize struct names to handle module-qualified vs unqualified names
                    // e.g., "MyGeometry.Point{Int64}" should equal "Point{Int64}"
                    (Value::Struct(a), Value::Struct(b)) => {
                        normalize_struct_name(&a.struct_name)
                            == normalize_struct_name(&b.struct_name)
                            && a.values.len() == b.values.len()
                            && format!("{:?}", a.values) == format!("{:?}", b.values)
                    }

                    // Tuples: structural equality (compare Debug representation)
                    (Value::Tuple(a), Value::Tuple(b)) => {
                        a.elements.len() == b.elements.len()
                            && format!("{:?}", a.elements) == format!("{:?}", b.elements)
                    }

                    // Expr: structural equality (head and args)
                    (Value::Expr(a), Value::Expr(b)) => {
                        a.head == b.head
                            && a.args.len() == b.args.len()
                            && format!("{:?}", a.args) == format!("{:?}", b.args)
                    }

                    // DataType: type identity by normalized name
                    // e.g., typeof(p) === Point{Int} should match Point{Int64}
                    (Value::DataType(a), Value::DataType(b)) => {
                        normalize_type_for_isa(&a.name()) == normalize_type_for_isa(&b.name())
                    }

                    // Different types: not identical
                    _ => false,
                };

                self.stack.push(Value::Bool(is_identical));
            }

            BuiltinId::Isequal => {
                // isequal(x, y) - NaN-aware equality
                // isequal(NaN, NaN) is true (unlike ==)
                // isequal(-0.0, 0.0) is false (unlike ==)
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                let is_equal = match (&left, &right) {
                    (Value::F64(a), Value::F64(b)) => {
                        if a.is_nan() && b.is_nan() {
                            true
                        } else {
                            a.to_bits() == b.to_bits() // Handles -0.0 vs 0.0
                        }
                    }
                    // For other types, compare by value
                    (Value::I64(a), Value::I64(b)) => a == b,
                    // Cross-type numeric equality: isequal(1, 1.0) is true
                    (Value::I64(a), Value::F64(b)) => {
                        if b.is_nan() {
                            false
                        } else {
                            (*a as f64) == *b && (*a as f64).to_bits() == b.to_bits()
                        }
                    }
                    (Value::F64(a), Value::I64(b)) => {
                        if a.is_nan() {
                            false
                        } else {
                            *a == (*b as f64) && a.to_bits() == (*b as f64).to_bits()
                        }
                    }
                    (Value::Str(a), Value::Str(b)) => a == b,
                    (Value::Char(a), Value::Char(b)) => a == b,
                    (Value::Nothing, Value::Nothing) => true,
                    (Value::Missing, Value::Missing) => true,
                    (Value::Tuple(a), Value::Tuple(b)) => {
                        a.elements.len() == b.elements.len()
                            && format!("{:?}", a.elements) == format!("{:?}", b.elements)
                    }
                    (Value::Struct(a), Value::Struct(b)) => {
                        // Normalize struct names to handle module-qualified vs unqualified
                        normalize_struct_name(&a.struct_name)
                            == normalize_struct_name(&b.struct_name)
                            && a.values.len() == b.values.len()
                            && format!("{:?}", a.values) == format!("{:?}", b.values)
                    }
                    // Expr: structural equality (head and args)
                    (Value::Expr(a), Value::Expr(b)) => {
                        a.head == b.head
                            && a.args.len() == b.args.len()
                            && format!("{:?}", a.args) == format!("{:?}", b.args)
                    }
                    // Array comparison: element-by-element equality
                    (Value::Array(a), Value::Array(b)) => {
                        let a_borrow = a.borrow();
                        let b_borrow = b.borrow();
                        if a_borrow.len() != b_borrow.len() || a_borrow.shape != b_borrow.shape {
                            false
                        } else {
                            // Compare element by element
                            let mut equal = true;
                            for i in 0..a_borrow.len() {
                                let av = a_borrow.data.get_value(i);
                                let bv = b_borrow.data.get_value(i);
                                match (av, bv) {
                                    (Some(Value::I64(x)), Some(Value::I64(y))) => {
                                        if x != y {
                                            equal = false;
                                            break;
                                        }
                                    }
                                    (Some(Value::F64(x)), Some(Value::F64(y))) => {
                                        // Use bit comparison for NaN handling
                                        if x.to_bits() != y.to_bits() && !(x.is_nan() && y.is_nan())
                                        {
                                            equal = false;
                                            break;
                                        }
                                    }
                                    (Some(Value::I64(x)), Some(Value::F64(y)))
                                    | (Some(Value::F64(y)), Some(Value::I64(x))) => {
                                        // Cross-type comparison
                                        if (x as f64) != y {
                                            equal = false;
                                            break;
                                        }
                                    }
                                    (Some(x), Some(y)) => {
                                        if format!("{:?}", x) != format!("{:?}", y) {
                                            equal = false;
                                            break;
                                        }
                                    }
                                    _ => {
                                        equal = false;
                                        break;
                                    }
                                }
                            }
                            equal
                        }
                    }
                    // Memory → Array (Issue #2764)
                    (Value::Memory(ma), Value::Memory(mb)) => {
                        let a = super::util::memory_to_array_ref(ma);
                        let b = super::util::memory_to_array_ref(mb);
                        let a_borrow = a.borrow();
                        let b_borrow = b.borrow();
                        if a_borrow.len() != b_borrow.len() || a_borrow.shape != b_borrow.shape {
                            false
                        } else {
                            let mut equal = true;
                            for i in 0..a_borrow.len() {
                                let av = a_borrow.data.get_value(i);
                                let bv = b_borrow.data.get_value(i);
                                match (av, bv) {
                                    (Some(Value::I64(x)), Some(Value::I64(y))) => {
                                        if x != y { equal = false; break; }
                                    }
                                    (Some(Value::F64(x)), Some(Value::F64(y))) => {
                                        if x.to_bits() != y.to_bits() && !(x.is_nan() && y.is_nan()) {
                                            equal = false; break;
                                        }
                                    }
                                    (Some(Value::I64(x)), Some(Value::F64(y)))
                                    | (Some(Value::F64(y)), Some(Value::I64(x))) => {
                                        if (x as f64) != y { equal = false; break; }
                                    }
                                    (Some(x), Some(y)) => {
                                        if format!("{:?}", x) != format!("{:?}", y) {
                                            equal = false; break;
                                        }
                                    }
                                    _ => { equal = false; break; }
                                }
                            }
                            equal
                        }
                    }
                    (Value::Memory(mem), Value::Array(b)) => {
                        // Memory → Array (Issue #2764)
                        let a = super::util::memory_to_array_ref(mem);
                        let a_borrow = a.borrow();
                        let b_borrow = b.borrow();
                        if a_borrow.len() != b_borrow.len() || a_borrow.shape != b_borrow.shape {
                            false
                        } else {
                            let mut equal = true;
                            for i in 0..a_borrow.len() {
                                let av = a_borrow.data.get_value(i);
                                let bv = b_borrow.data.get_value(i);
                                match (av, bv) {
                                    (Some(Value::I64(x)), Some(Value::I64(y))) => {
                                        if x != y { equal = false; break; }
                                    }
                                    (Some(Value::F64(x)), Some(Value::F64(y))) => {
                                        if x.to_bits() != y.to_bits() && !(x.is_nan() && y.is_nan()) {
                                            equal = false; break;
                                        }
                                    }
                                    (Some(Value::I64(x)), Some(Value::F64(y)))
                                    | (Some(Value::F64(y)), Some(Value::I64(x))) => {
                                        if (x as f64) != y { equal = false; break; }
                                    }
                                    (Some(x), Some(y)) => {
                                        if format!("{:?}", x) != format!("{:?}", y) {
                                            equal = false; break;
                                        }
                                    }
                                    _ => { equal = false; break; }
                                }
                            }
                            equal
                        }
                    }
                    (Value::Array(a), Value::Memory(mem)) => {
                        // Memory → Array (Issue #2764)
                        let b = super::util::memory_to_array_ref(mem);
                        let a_borrow = a.borrow();
                        let b_borrow = b.borrow();
                        if a_borrow.len() != b_borrow.len() || a_borrow.shape != b_borrow.shape {
                            false
                        } else {
                            let mut equal = true;
                            for i in 0..a_borrow.len() {
                                let av = a_borrow.data.get_value(i);
                                let bv = b_borrow.data.get_value(i);
                                match (av, bv) {
                                    (Some(Value::I64(x)), Some(Value::I64(y))) => {
                                        if x != y { equal = false; break; }
                                    }
                                    (Some(Value::F64(x)), Some(Value::F64(y))) => {
                                        if x.to_bits() != y.to_bits() && !(x.is_nan() && y.is_nan()) {
                                            equal = false; break;
                                        }
                                    }
                                    (Some(Value::I64(x)), Some(Value::F64(y)))
                                    | (Some(Value::F64(y)), Some(Value::I64(x))) => {
                                        if (x as f64) != y { equal = false; break; }
                                    }
                                    (Some(x), Some(y)) => {
                                        if format!("{:?}", x) != format!("{:?}", y) {
                                            equal = false; break;
                                        }
                                    }
                                    _ => { equal = false; break; }
                                }
                            }
                            equal
                        }
                    }
                    // Different types are not equal
                    _ => false,
                };

                self.stack.push(Value::Bool(is_equal));
            }

            BuiltinId::Hash => {
                // hash(x) - compute hash value
                let val = self.stack.pop_value()?;
                let mut hasher = DefaultHasher::new();

                match &val {
                    Value::I64(v) => v.hash(&mut hasher),
                    Value::F64(v) => v.to_bits().hash(&mut hasher),
                    Value::Str(s) => s.hash(&mut hasher),
                    Value::Char(c) => c.hash(&mut hasher),
                    Value::Nothing => 0u64.hash(&mut hasher),
                    Value::Missing => 1u64.hash(&mut hasher), // Different hash from Nothing
                    Value::Tuple(t) => {
                        for v in &t.elements {
                            format!("{:?}", v).hash(&mut hasher);
                        }
                    }
                    Value::Array(arr) => {
                        // Hash array by contents
                        let arr_borrow = arr.borrow();
                        for i in 0..arr_borrow.len() {
                            if let Some(v) = arr_borrow.data.get_value(i) {
                                format!("{:?}", v).hash(&mut hasher);
                            }
                        }
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = super::util::memory_to_array_ref(mem);
                        let arr_borrow = arr.borrow();
                        for i in 0..arr_borrow.len() {
                            if let Some(v) = arr_borrow.data.get_value(i) {
                                format!("{:?}", v).hash(&mut hasher);
                            }
                        }
                    }
                    _ => {
                        // For other types, hash the debug representation
                        format!("{:?}", val).hash(&mut hasher);
                    }
                }

                self.stack.push(Value::I64(hasher.finish() as i64));
            }

            BuiltinId::_Hash => {
                // _hash(x) - internal intrinsic for hash computation (Issue #2582)
                // Same logic as Hash builtin, used by Pure Julia hash methods in hashing.jl
                let val = self.stack.pop_value()?;
                let mut hasher = DefaultHasher::new();

                match &val {
                    Value::I64(v) => v.hash(&mut hasher),
                    Value::F64(v) => v.to_bits().hash(&mut hasher),
                    Value::Str(s) => s.hash(&mut hasher),
                    Value::Char(c) => c.hash(&mut hasher),
                    Value::Bool(b) => b.hash(&mut hasher),
                    Value::Nothing => 0u64.hash(&mut hasher),
                    Value::Missing => 1u64.hash(&mut hasher),
                    Value::Tuple(t) => {
                        for v in &t.elements {
                            format!("{:?}", v).hash(&mut hasher);
                        }
                    }
                    Value::Array(arr) => {
                        let arr_borrow = arr.borrow();
                        for i in 0..arr_borrow.len() {
                            if let Some(v) = arr_borrow.data.get_value(i) {
                                format!("{:?}", v).hash(&mut hasher);
                            }
                        }
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = super::util::memory_to_array_ref(mem);
                        let arr_borrow = arr.borrow();
                        for i in 0..arr_borrow.len() {
                            if let Some(v) = arr_borrow.data.get_value(i) {
                                format!("{:?}", v).hash(&mut hasher);
                            }
                        }
                    }
                    _ => {
                        format!("{:?}", val).hash(&mut hasher);
                    }
                }

                self.stack.push(Value::I64(hasher.finish() as i64));
            }

            BuiltinId::Isless => {
                // isless(x, y) - strict weak ordering for sorting
                // isless is used by sort() and defines a total order.
                // Key properties:
                // - isless(NaN, x) = false for all x (NaN is not less than anything)
                // - isless(x, NaN) = true for all non-NaN x (everything is less than NaN)
                // - isless(missing, x) = false for all x (missing is not less than anything)
                // - isless(x, missing) = true for all non-missing x (everything is less than missing)
                // This places NaN and missing at the end when sorting.
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                let is_less = match (&left, &right) {
                    // Missing handling (Missing sorts to the end)
                    (Value::Missing, _) => false, // missing is not less than anything
                    (_, Value::Missing) => true,  // everything is less than missing
                    // NaN handling (NaN sorts to the end, but before missing conceptually)
                    (Value::F64(a), Value::F64(b)) => {
                        if a.is_nan() {
                            false // NaN is not less than anything
                        } else if b.is_nan() {
                            true // non-NaN is less than NaN
                        } else {
                            a < b
                        }
                    }
                    // Integer comparison
                    (Value::I64(a), Value::I64(b)) => a < b,
                    // Cross-type numeric comparison
                    (Value::I64(a), Value::F64(b)) => {
                        if b.is_nan() {
                            true // non-NaN is less than NaN
                        } else {
                            (*a as f64) < *b
                        }
                    }
                    (Value::F64(a), Value::I64(b)) => {
                        if a.is_nan() {
                            false // NaN is not less than anything
                        } else {
                            *a < (*b as f64)
                        }
                    }
                    // String lexicographic comparison
                    (Value::Str(a), Value::Str(b)) => a < b,
                    // Char comparison
                    (Value::Char(a), Value::Char(b)) => a < b,
                    // Bool comparison (false < true)
                    (Value::Bool(a), Value::Bool(b)) => !a && *b,
                    // Nothing handling (nothing sorts before values)
                    (Value::Nothing, Value::Nothing) => false, // nothing is not less than itself
                    (Value::Nothing, _) => true, // nothing is less than everything except itself
                    (_, Value::Nothing) => false, // nothing is not less than non-nothing
                    // Default: types without defined ordering return false
                    _ => false,
                };

                self.stack.push(Value::Bool(is_less));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
