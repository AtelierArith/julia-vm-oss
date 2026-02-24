//! Array function compilation.
//!
//! Handles compilation of Julia array functions:
//! - zeros(dims...): Create array of zeros
//! - zeros(Type, dims...): Create typed array of zeros
//! - ones(dims...): Create array of ones
//! - ones(Type, dims...): Create typed array of ones
//! - length(collection): Get length of collection
//! - getindex(collection, indices...): Index into collection
//! - setindex!(collection, value, indices...): Indexed assignment

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::super::{err, CResult, CoreCompiler};

impl CoreCompiler<'_> {
    /// Compile array function calls.
    /// Returns Some(type) if handled, None if not an array function.
    /// Note: zeros/ones with type parameter are handled via Expr::Builtin path in builtin.rs
    pub(in super::super) fn compile_builtin_array(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        match name {
            "zeros" => {
                // zeros(dims...) - via Builtin (default Float64)
                // Note: zeros(Type, dims...) is handled in compile_builtin_expr
                for dim in args {
                    self.compile_expr_as(dim, ValueType::I64)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Zeros, args.len()));
                Ok(Some(ValueType::Array))
            }
            "ones" => {
                // ones(dims...) - via Builtin (default Float64)
                // Note: ones(Type, dims...) is handled in compile_builtin_expr
                for dim in args {
                    self.compile_expr_as(dim, ValueType::I64)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Ones, args.len()));
                Ok(Some(ValueType::Array))
            }
            "similar" => {
                // similar(a) - uninitialized array with same type and shape
                // similar(a, n) - uninitialized array with same type, length n (Issue #2129)
                if args.is_empty() || args.len() > 2 {
                    return err(
                        "similar requires 1 or 2 arguments: similar(array) or similar(array, n)",
                    );
                }
                self.compile_expr(&args[0])?;
                if args.len() == 2 {
                    self.compile_expr_as(&args[1], ValueType::I64)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Similar, args.len()));
                let arr_ty = self.infer_expr_type(&args[0]);
                Ok(Some(arr_ty))
            }
            "length" => {
                // Universal length - handles Array, Tuple, Dict, Range, String via CallBuiltin
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
                Ok(Some(ValueType::I64))
            }
            "getindex" => {
                // getindex(collection, indices...) - Julia-compliant indexing
                // s[i] is lowered to getindex(s, i)
                if args.is_empty() {
                    return err(
                        "getindex requires at least 1 argument: getindex(collection, indices...)",
                    );
                }

                let collection_type = self.infer_expr_type(&args[0]);

                match collection_type {
                    ValueType::Dict => {
                        // Dict indexing: getindex(d, key)
                        if args.len() != 2 {
                            return err("Dict indexing requires exactly one key");
                        }
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::DictGet, 2));
                        Ok(Some(ValueType::Any))
                    }
                    ValueType::Tuple => {
                        // Tuple indexing: getindex(t, i)
                        if args.len() != 2 {
                            return err("Tuple indexing requires exactly one index");
                        }
                        self.compile_expr(&args[0])?;
                        self.compile_expr_as(&args[1], ValueType::I64)?;
                        self.emit(Instr::TupleGet);
                        Ok(Some(ValueType::Any))
                    }
                    ValueType::NamedTuple => {
                        // NamedTuple indexing: getindex(nt, i) or getindex(nt, :symbol)
                        // Julia supports both integer index and symbol index for NamedTuples
                        if args.len() != 2 {
                            return err("NamedTuple indexing requires exactly one index");
                        }
                        let index_type = self.infer_expr_type(&args[1]);
                        self.compile_expr(&args[0])?;
                        match index_type {
                            ValueType::Symbol => {
                                // Symbol index: nt[:field]
                                self.compile_expr(&args[1])?;
                                self.emit(Instr::NamedTupleGetBySymbol);
                                Ok(Some(ValueType::Any))
                            }
                            _ => {
                                // Integer index: nt[1]
                                self.compile_expr_as(&args[1], ValueType::I64)?;
                                self.emit(Instr::NamedTupleGetIndex);
                                Ok(Some(ValueType::Any))
                            }
                        }
                    }
                    ValueType::Pairs => {
                        // Base.Pairs indexing: getindex(pairs, :symbol)
                        // Only symbol index is supported (kwargs[:key])
                        if args.len() != 2 {
                            return err("Pairs indexing requires exactly one index");
                        }
                        let index_type = self.infer_expr_type(&args[1]);
                        self.compile_expr(&args[0])?;
                        match index_type {
                            ValueType::Symbol => {
                                // Symbol index: kwargs[:key]
                                self.compile_expr(&args[1])?;
                                self.emit(Instr::PairsGetBySymbol);
                                Ok(Some(ValueType::Any))
                            }
                            _ => err("Base.Pairs only supports Symbol indexing (kwargs[:key])"),
                        }
                    }
                    ValueType::Str => {
                        // String indexing: getindex(s, i) or getindex(s, range)
                        if args.len() != 2 {
                            return err("String indexing requires exactly one index");
                        }
                        let is_range =
                            matches!(&args[1], Expr::Range { .. } | Expr::SliceAll { .. });
                        self.compile_expr(&args[0])?;
                        if is_range {
                            // String slicing: s[2:4] returns String
                            self.compile_expr(&args[1])?;
                            self.emit(Instr::IndexSlice(1));
                            Ok(Some(ValueType::Str))
                        } else {
                            // String indexing: s[i] returns Char
                            self.compile_expr_as(&args[1], ValueType::I64)?;
                            self.emit(Instr::IndexLoad(1));
                            Ok(Some(ValueType::Char))
                        }
                    }
                    _ => {
                        // Array or unknown type - use IndexLoad/IndexSlice
                        let indices = &args[1..];
                        // Check for slice-like indices: Range, SliceAll, or Array (for logical indexing)
                        // Bool is included because broadcast comparisons (arr .> 2) may be
                        // inferred as Bool when the result is actually a Bool array (Issue #2694)
                        let has_slice = indices.iter().any(|idx| {
                            match idx {
                                Expr::Range { .. } | Expr::SliceAll { .. } => true,
                                _ => {
                                    // Array index could be logical indexing (bool array) or index array
                                    let idx_type = self.infer_expr_type(idx);
                                    matches!(
                                        idx_type,
                                        ValueType::Array | ValueType::ArrayOf(_) | ValueType::Bool
                                    )
                                }
                            }
                        });

                        self.compile_expr(&args[0])?;
                        for idx in indices {
                            match idx {
                                Expr::Range { .. } | Expr::SliceAll { .. } => {
                                    self.compile_expr(idx)?;
                                }
                                _ => {
                                    // Check if index might be a CartesianIndex (struct type), Array,
                                    // Bool array, or non-numeric key for Dict indexing (Issue #1814)
                                    let idx_type = self.infer_expr_type(idx);
                                    if matches!(
                                        idx_type,
                                        ValueType::Struct(_)
                                            | ValueType::Any
                                            | ValueType::Array
                                            | ValueType::ArrayOf(_)
                                            | ValueType::Bool
                                            | ValueType::Str
                                            | ValueType::Symbol
                                    ) {
                                        self.compile_expr(idx)?;
                                    } else {
                                        self.compile_expr_as(idx, ValueType::I64)?;
                                    }
                                }
                            }
                        }
                        if has_slice {
                            self.emit(Instr::IndexSlice(indices.len()));
                            Ok(Some(ValueType::Any))
                        } else {
                            self.emit(Instr::IndexLoad(indices.len()));
                            Ok(Some(ValueType::Any))
                        }
                    }
                }
            }
            "setindex!" => {
                // setindex!(collection, value, indices...) - Julia-compliant indexed assignment
                // s[i] = v is lowered to setindex!(s, v, i)
                if args.len() < 3 {
                    return err("setindex! requires at least 3 arguments: setindex!(collection, value, indices...)");
                }

                let collection_type = self.infer_expr_type(&args[0]);

                match collection_type {
                    ValueType::Dict => {
                        // Dict assignment: setindex!(d, value, key)
                        self.compile_expr(&args[0])?; // dict
                        self.compile_expr(&args[2])?; // key
                        self.compile_expr(&args[1])?; // value
                        self.emit(Instr::CallBuiltin(BuiltinId::DictSet, 3));
                        // DictSet modifies dict in place via Rc reference
                        // Pop the modified dict and return the value
                        self.emit(Instr::Pop);
                        self.compile_expr(&args[1])?;
                        Ok(Some(ValueType::Any))
                    }
                    _ => {
                        // Array or unknown type assignment: setindex!(collection, value, indices...)
                        // When collection type is Any and index is non-numeric (e.g., Str, Symbol),
                        // emit DictSet for runtime Dict dispatch (Issue #1814)
                        let idx_types: Vec<_> = args[2..]
                            .iter()
                            .map(|idx| self.infer_expr_type(idx))
                            .collect();
                        let has_non_numeric_idx = args.len() == 3
                            && idx_types
                                .iter()
                                .any(|t| matches!(t, ValueType::Str | ValueType::Symbol));

                        if has_non_numeric_idx {
                            // Likely Dict assignment: emit DictSet
                            self.compile_expr(&args[0])?; // dict
                            self.compile_expr(&args[2])?; // key
                            self.compile_expr(&args[1])?; // value
                            self.emit(Instr::CallBuiltin(BuiltinId::DictSet, 3));
                            self.emit(Instr::Pop);
                            self.compile_expr(&args[1])?;
                            Ok(Some(ValueType::Any))
                        } else {
                            // IndexStore expects stack: [array, idx1, idx2, ..., value] with value on top
                            // For Any-typed indices, compile without I64 coercion so the VM
                            // can dispatch to Dict at runtime (Issue #1814)
                            self.compile_expr(&args[0])?; // array (bottom)
                            for idx in &args[2..] {
                                let idx_type = self.infer_expr_type(idx);
                                if matches!(idx_type, ValueType::Any) {
                                    self.compile_expr(idx)?; // preserve Any type
                                } else {
                                    self.compile_expr_as(idx, ValueType::I64)?; // indices
                                }
                            }
                            self.compile_expr(&args[1])?; // value (top)
                            self.emit(Instr::IndexStore(args.len() - 2));
                            // IndexStore modifies array in place (via Rc<RefCell>) and leaves it on stack
                            // Pop the modified array and push Nothing as return value
                            self.emit(Instr::Pop);
                            self.emit(Instr::PushNothing);
                            Ok(Some(ValueType::Nothing))
                        }
                    }
                }
            }
            _ => Ok(None),
        }
    }
}
