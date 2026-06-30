//! Struct compilation (constructors and field access).

use crate::builtins::BuiltinId;
use crate::ir::core::{Expr, Literal};
use crate::types::JuliaType;
use crate::vm::value::is_array_wrapper_struct_name;
use crate::vm::{Instr, ValueType};

use super::super::{
    err, extract_module_path_from_expr, get_base_exported_constant_value, get_math_constant_value,
    is_stdlib_module, CResult, CoreCompiler, StructInfo,
};

const EXPR_FIELD_HEAD_INDEX: usize = 0;
const EXPR_FIELD_ARGS_INDEX: usize = 1;
const LINE_NUMBER_NODE_FIELD_LINE_INDEX: usize = 0;
const LINE_NUMBER_NODE_FIELD_FILE_INDEX: usize = 1;
const GLOBAL_REF_FIELD_MODULE_INDEX: usize = 0;
const GLOBAL_REF_FIELD_NAME_INDEX: usize = 1;

fn is_array_wrapper_compat_field(field: &str) -> bool {
    matches!(field, "_mem" | "_size")
}

impl CoreCompiler<'_> {
    /// Compile a struct constructor call: Point(1.0, 2.0)
    pub(in super::super) fn compile_struct_constructor(
        &mut self,
        struct_info: StructInfo,
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Check that argument count matches field count
        if args.len() != struct_info.fields.len() {
            return err(format!(
                "Struct constructor expects {} arguments, got {}",
                struct_info.fields.len(),
                args.len()
            ));
        }

        // The precise per-field `JuliaType` list preserves declared widths such
        // as `UInt64` / `Int32` that the `ValueType` field list collapses to
        // `I64` (Issue #4990). Used to decide which fields must defer coercion
        // to the runtime `NewStruct` step instead of clobbering the value here.
        let precise_field_types: Vec<crate::types::JuliaType> = self
            .shared_ctx
            .field_julia_types_by_type_id(struct_info.type_id)
            .map(<[_]>::to_vec)
            .unwrap_or_default();

        // Compile each argument with the expected field type
        // For Any-typed fields and Function-typed fields, don't coerce - preserve the original type
        for (idx, (arg, (_, field_ty))) in args.iter().zip(struct_info.fields.iter()).enumerate() {
            let precise = precise_field_types.get(idx);
            if *field_ty == ValueType::Any || *field_ty == ValueType::Function {
                // Any-typed or Function-typed fields: compile without type coercion
                // Function fields accept any callable (functions, composed functions, etc.)
                self.compile_expr(arg)?;
            } else if precise.is_some_and(field_type_is_abstract_numeric) {
                // Abstract numeric field types (`Real`, `Number`, `Integer`, ...)
                // — including the case where a struct type parameter is bound to
                // an abstract type, e.g. `Foo{Real}(1)`. Julia's default
                // constructor inserts `convert(fieldtype, x)`, which is a no-op
                // when `x isa fieldtype`, so the *original concrete* value is
                // preserved (`Foo{Real}(1).x === 1`, an `Int64`; not `1.0`).
                //
                // `julia_type_to_value_type` lossily maps these abstract types
                // to a concrete `ValueType::F64`/`I64`, so a `compile_expr_as`
                // coercion here would clobber the value's real type/width. Leave
                // the argument untouched (Issue #5060).
                self.compile_expr(arg)?;
            } else if precise.is_some_and(field_type_needs_runtime_coercion) {
                // Narrow/unsigned integer or non-Float64 float fields: the
                // `ValueType` representation collapses widths (e.g. `UInt64` ->
                // `I64`), so a compile-time `compile_expr_as` coercion would
                // emit a `DynamicToI64`/etc. that destroys the declared width.
                // Compile the argument as-is and let the runtime `NewStruct`
                // step apply the precise `convert(fieldtype, x)` (Issue #4990).
                self.compile_expr(arg)?;
            } else {
                // Typed fields: compile with type coercion
                self.compile_expr_as(arg, field_ty.clone())?;
            }
        }

        // Emit NewStruct instruction
        self.emit(Instr::NewStruct(struct_info.type_id, args.len()));

        Ok(ValueType::Struct(struct_info.type_id))
    }

    /// Side-effect-free convertibility pre-check for the field-count default
    /// constructor fallback (Issue #7793 regression guard).
    ///
    /// [`Self::compile_struct_constructor`] coerces only a *subset* of fields via
    /// [`Self::compile_expr_as`] — `Any`/`Function`, abstract-numeric, and
    /// narrow/unsigned-runtime-coercion fields are compiled as-is and never raise
    /// the compile-time `Cannot convert ...` error. This mirrors that exact branch
    /// selection and, for *only* the fields that would go through `compile_expr_as`,
    /// checks `coercion_accepts`. Returns `false` iff any such field's argument
    /// type is NOT statically coercible to the field type — i.e. iff
    /// `compile_struct_constructor` would raise an (uncatchable) compile error.
    ///
    /// Caller uses this BEFORE `compile_struct_constructor` so a non-matching
    /// arg-type call falls through to normal dispatch (catchable `MethodError`),
    /// matching upstream Julia, instead of manufacturing the default constructor
    /// and erroring at compile time. The check emits NOTHING.
    pub(in super::super) fn struct_field_count_ctor_args_convertible(
        &mut self,
        struct_info: &StructInfo,
        args: &[Expr],
    ) -> bool {
        if args.len() != struct_info.fields.len() {
            return false;
        }
        let precise_field_types: Vec<crate::types::JuliaType> = self
            .shared_ctx
            .field_julia_types_by_type_id(struct_info.type_id)
            .map(<[_]>::to_vec)
            .unwrap_or_default();

        for (idx, (arg, (_, field_ty))) in args.iter().zip(struct_info.fields.iter()).enumerate() {
            let precise = precise_field_types.get(idx);
            // Mirror `compile_struct_constructor`'s branch selection: only the
            // final `else` branch (plain concrete typed fields) calls
            // `compile_expr_as` and can produce "Cannot convert". The other
            // branches compile the argument as-is and accept any value type.
            let goes_through_compile_expr_as = *field_ty != ValueType::Any
                && *field_ty != ValueType::Function
                && !precise.is_some_and(field_type_is_abstract_numeric)
                && !precise.is_some_and(field_type_needs_runtime_coercion);
            if goes_through_compile_expr_as {
                let actual = self.infer_expr_type(arg);
                if matches!(actual, ValueType::Any) {
                    continue;
                }
                if !self.coercion_accepts(&actual, field_ty) {
                    return false;
                }
            }
        }
        true
    }

    /// Resolve a dotted qualified path to the canonical name of a known user
    /// (sub)module, or `None` when the path does not name one (Issue #8113/#8114).
    ///
    /// The path is accepted as-is when it is a registered module
    /// (`Outer.Inner`). Otherwise its root segment is resolved through the
    /// module-alias table so an alias-rooted path resolves to its underlying
    /// module (`MA.Inner` with `const MA = Outer` -> `Outer.Inner`); the
    /// remaining segments are preserved, so deeper chains resolve too.
    fn resolve_user_module_path(&self, path: &str) -> Option<String> {
        // Canonicalize via the shared alias resolver first (resolving an
        // alias-rooted path like `AA.B.C` with `const AA = A` to `A.B.C`) BEFORE
        // the known-module lookup. Resolving first avoids matching a propagated
        // alias spelling (`AA.B.C` can appear in `module_exports`) that
        // `compile_module_function_ref` cannot then resolve.
        let canonical = self.resolve_module_alias_path(path);
        if self.module_functions.contains_key(&canonical)
            || self.module_exports.contains_key(&canonical)
        {
            Some(canonical)
        } else {
            None
        }
    }

    /// Whether the leftmost identifier of a (possibly nested) field-access path
    /// is a non-module local binding that shadows a same-named module, mirroring
    /// the single-level shadow check in [`Self::compile_field_access`]
    /// (Issue #7245). Used to keep a local parameter named like a module from
    /// being mis-resolved as a qualified module access.
    fn module_path_root_shadowed_by_local(&self, object: &Expr) -> bool {
        let mut current = object;
        loop {
            match current {
                Expr::Var(name, _) => {
                    return self
                        .locals
                        .get(name)
                        .is_some_and(|ty| !matches!(ty, ValueType::Module));
                }
                Expr::FieldAccess { object, .. } => current = object,
                _ => return false,
            }
        }
    }

    /// Compile a field access: obj.field
    pub(in super::super) fn compile_field_access(
        &mut self,
        object: &Expr,
        field: &str,
    ) -> CResult<ValueType> {
        // Check for nested module path like Base.MathConstants.e
        if let Some(module_path) = extract_module_path_from_expr(object) {
            // Check if this is Base.MathConstants constant access
            if module_path == "Base.MathConstants" {
                if let Some(value) = get_math_constant_value(field) {
                    self.emit(Instr::PushF64(value));
                    return Ok(ValueType::F64);
                }
                return err(format!(
                    "Base.MathConstants has no constant named {}",
                    field
                ));
            }

            if module_path == "Sys" && field == "WORD_SIZE" {
                self.emit(Instr::PushI64(i64::from(usize::BITS)));
                return Ok(ValueType::I64);
            }

            // Handle Base module constants (only pi, ℯ, Inf, NaN are exported from Base)
            // Other MathConstants like 'e', 'golden', 'eulergamma' require Base.MathConstants.e
            if module_path == "Base" {
                if let Some(value) = get_base_exported_constant_value(field) {
                    self.emit(Instr::PushF64(value));
                    return Ok(ValueType::F64);
                }
            }

            // Handle other Base submodules or module function refs. Base preload
            // submodules can be represented as top-level IR modules, so
            // canonicalize before the legacy Base-submodule fallback (Issue #8269).
            let canonical_module_path = self.canonical_module_path(&module_path);
            if canonical_module_path != module_path
                || module_path.starts_with("Base.")
                || is_stdlib_module(&module_path)
            {
                return self.compile_module_function_ref(&canonical_module_path, field);
            }

            // A multi-segment qualified path that names a known user (sub)module
            // — e.g. `Outer.Inner` in `Outer.Inner.T1` (Issue #8113), or an
            // alias-rooted `MA.Inner` where `const MA = Outer`. The intermediate
            // `Inner` resolves to a `Module` value, so without this the field
            // access would compile the object to a `Module` and fail with
            // "Field access requires a struct type, got Module". A single-segment
            // module name (object is a bare `Var`) is intentionally left to the
            // `Expr::Var` branch below so a same-named local can still shadow the
            // module (Issue #7245); here we only handle dotted paths and guard the
            // path root against a non-module local shadow.
            if module_path.contains('.') && !self.module_path_root_shadowed_by_local(object) {
                if let Some(resolved_path) = self.resolve_user_module_path(&module_path) {
                    return self.compile_module_function_ref(&resolved_path, field);
                }
            }
        }

        if let Expr::Var(module_name, _) = object {
            let local_ty = self.locals.get(module_name).cloned();
            // A local binding (function parameter / local variable) shadows a
            // same-named module in scope, matching Julia's scoping rules. Without
            // this, a method like `f(D::Diagonal) = D.diag[i]` defined inside a
            // user module literally named `D` mis-resolves the field access
            // `D.diag` as the module-qualified call `D.diag(...)` and fails with
            // "Module D has no function named diag" (Issue #7245). The lone
            // exception is a local that actually holds a module value, which is
            // still a module access.
            let shadowed_by_local =
                local_ty.is_some() && !matches!(local_ty, Some(ValueType::Module));
            let is_module_value = !shadowed_by_local
                && (matches!(local_ty, Some(ValueType::Module))
                    || self
                        .nested_module_path_in_current_module(module_name)
                        .is_some()
                    || is_stdlib_module(module_name)
                    || self.module_aliases.contains_key(module_name)
                    || self.module_functions.contains_key(module_name));

            if is_module_value {
                let resolved_module = if let Some(module_path) =
                    self.nested_module_path_in_current_module(module_name)
                {
                    module_path
                } else {
                    self.module_aliases
                        .get(module_name)
                        .cloned()
                        .unwrap_or_else(|| module_name.clone())
                };
                let resolved_module = self.canonical_module_path(&resolved_module);

                // Handle Base module constants (pi, e, Inf, NaN, etc.)
                // These are exported from Base.MathConstants but accessible as Base.pi
                if resolved_module == "Base" {
                    if let Some(value) = get_math_constant_value(field) {
                        self.emit(Instr::PushF64(value));
                        return Ok(ValueType::F64);
                    }
                }
                if resolved_module == "Sys" && field == "WORD_SIZE" {
                    self.emit(Instr::PushI64(i64::from(usize::BITS)));
                    return Ok(ValueType::I64);
                }

                return self.compile_module_function_ref(&resolved_module, field);
            }
        }

        // Issue #8127: a user-defined `getproperty` override intercepts *all*
        // property access on its receiver type. In Julia `x.f` always lowers to
        // `getproperty(x, :f)`, whose default falls back to `getfield`; user
        // overloads are how types expose computed properties (e.g. a wrapper that
        // stores one field and surfaces derived `.x`/`.y`/`.z`). When the object's
        // static type is a struct for which dispatch resolves `getproperty` to a
        // *user* method, route the access through that method instead of the
        // compile-time declared-field lookup, so both computed and declared
        // properties go through the override. Non-overridden types keep the fast
        // direct-field path below.
        let obj_julia_type = self.infer_julia_type(object);
        if self.struct_type_has_user_getproperty_override(&obj_julia_type) {
            return self.compile_getproperty_override_call(object, field);
        }

        // Compile the object expression
        let obj_ty = self.compile_expr(object)?;

        match obj_ty {
            ValueType::Struct(type_id) => {
                // Look up the struct definition and find field info
                let mut result: Option<(usize, ValueType)> = None;
                let mut struct_name = String::new();

                for (name, struct_info) in self.shared_ctx.struct_table.iter() {
                    if struct_info.type_id == type_id {
                        struct_name = name.clone();
                        for (idx, (field_name, field_ty)) in struct_info.fields.iter().enumerate() {
                            if field_name == field {
                                result = Some((idx, field_ty.clone()));
                                break;
                            }
                        }
                        break;
                    }
                }

                match result {
                    Some((idx, field_ty)) => {
                        self.emit(Instr::GetField(idx));
                        Ok(field_ty)
                    }
                    None => {
                        if is_array_wrapper_struct_name(&struct_name)
                            && is_array_wrapper_compat_field(field)
                        {
                            self.emit(Instr::GetFieldByName(field.to_string()));
                            Ok(ValueType::Any)
                        } else if struct_name.is_empty() {
                            err(format!("Unknown struct type_id: {}", type_id))
                        } else {
                            err(format!(
                                "Unknown field '{}' on struct '{}'",
                                field, struct_name
                            ))
                        }
                    }
                }
            }
            ValueType::Any => {
                // For Any type, first check for special builtin type fields
                // Expr, LineNumberNode, GlobalRef have predefined fields that need runtime dispatch
                // These are checked before user-defined struct fields to support metaprogramming
                match field {
                    // Expr fields: head, args
                    "head" | "args" => {
                        // Emit dynamic field access that works for Expr at runtime
                        let field_idx = if field == "head" {
                            EXPR_FIELD_HEAD_INDEX
                        } else {
                            EXPR_FIELD_ARGS_INDEX
                        };
                        self.emit(Instr::GetExprField(field_idx));
                        return Ok(ValueType::Any);
                    }
                    _ => {}
                }

                // For user-defined structs, use runtime field lookup by name.
                // This is necessary because different structs may have the same field name
                // at different indices (e.g., DomainError.msg at index 1, DimensionMismatch.msg at index 0).
                // The GetFieldByName instruction looks up the field index at runtime using
                // the struct's type_id to find the correct definition.
                self.emit(Instr::GetFieldByName(field.to_string()));
                // Return Any since we don't know the concrete struct type at compile time.
                // The actual field type depends on the runtime struct instance.
                Ok(ValueType::Any)
            }
            ValueType::DataType => match field {
                "parameters" => {
                    self.emit(Instr::CallBuiltin(BuiltinId::_TypeParameters, 1));
                    Ok(ValueType::Tuple)
                }
                "var" => {
                    self.emit(Instr::CallBuiltin(BuiltinId::_UnionAllVar, 1));
                    Ok(ValueType::DataType)
                }
                "body" => {
                    self.emit(Instr::CallBuiltin(BuiltinId::_UnionAllBody, 1));
                    Ok(ValueType::DataType)
                }
                "name" => {
                    self.emit(Instr::GetFieldByName(field.to_string()));
                    Ok(ValueType::Any)
                }
                "lb" => {
                    self.emit(Instr::CallBuiltin(BuiltinId::_TypeVarLowerBound, 1));
                    Ok(ValueType::DataType)
                }
                "ub" => {
                    self.emit(Instr::CallBuiltin(BuiltinId::_TypeVarUpperBound, 1));
                    Ok(ValueType::DataType)
                }
                _ => err(format!("type DataType has no field {}", field)),
            },
            // Expr type has special fields: head (Symbol) and args (Vector{Any})
            // This matches Julia's Core.Expr structure
            ValueType::Expr => {
                match field {
                    "head" => {
                        self.emit(Instr::GetExprField(EXPR_FIELD_HEAD_INDEX));
                        Ok(ValueType::Symbol)
                    }
                    "args" => {
                        self.emit(Instr::GetExprField(EXPR_FIELD_ARGS_INDEX));
                        Ok(ValueType::Array) // Vector{Any}
                    }
                    _ => {
                        // Workaround: defer invalid macro helper field access guarded by `isa(x, QuoteNode)` (Issue #7535).
                        // A compile-time macro argument can specialize as Expr
                        // even when another branch first checks for QuoteNode.
                        // Let runtime dispatch reject only the executed path.
                        self.emit(Instr::GetFieldByName(field.to_string()));
                        Ok(ValueType::Any)
                    }
                }
            }
            // LineNumberNode type has special fields: line (Int64) and file (Symbol)
            // This matches Julia's LineNumberNode structure
            ValueType::LineNumberNode => {
                match field {
                    "line" => {
                        self.emit(Instr::GetLineNumberNodeField(
                            LINE_NUMBER_NODE_FIELD_LINE_INDEX,
                        ));
                        Ok(ValueType::I64)
                    }
                    "file" => {
                        self.emit(Instr::GetLineNumberNodeField(
                            LINE_NUMBER_NODE_FIELD_FILE_INDEX,
                        ));
                        Ok(ValueType::Symbol) // Returns Symbol (or nothing if no file)
                    }
                    _ => {
                        self.emit(Instr::GetFieldByName(field.to_string()));
                        Ok(ValueType::Any)
                    }
                }
            }
            // QuoteNode type has special field: value (the wrapped value)
            // This matches Julia's QuoteNode structure
            ValueType::QuoteNode => {
                match field {
                    "value" => {
                        self.emit(Instr::GetQuoteNodeValue);
                        Ok(ValueType::Any) // The wrapped value can be any type
                    }
                    _ => {
                        self.emit(Instr::GetFieldByName(field.to_string()));
                        Ok(ValueType::Any)
                    }
                }
            }
            // GlobalRef type has special fields: mod (Module) and name (Symbol)
            // This matches Julia's GlobalRef structure
            ValueType::GlobalRef => match field {
                "mod" => {
                    self.emit(Instr::GetGlobalRefField(GLOBAL_REF_FIELD_MODULE_INDEX));
                    Ok(ValueType::Module)
                }
                "name" => {
                    self.emit(Instr::GetGlobalRefField(GLOBAL_REF_FIELD_NAME_INDEX));
                    Ok(ValueType::Symbol)
                }
                _ => {
                    self.emit(Instr::GetFieldByName(field.to_string()));
                    Ok(ValueType::Any)
                }
            },
            // NamedTuple field access: nt.field
            // Julia supports both nt.field and nt[:field] for NamedTuples
            ValueType::NamedTuple => {
                self.emit(Instr::NamedTupleGetField(field.to_string()));
                Ok(ValueType::Any)
            }
            // Base.Pairs does NOT support dot notation - must use kwargs[:field]
            // This matches Julia's behavior where kwargs.field is an error
            ValueType::Pairs => err(format!(
                "type Base.Pairs has no field `{}`. Use kwargs[:{}] instead",
                field, field
            )),
            // For F64 and other types that might actually be structs at runtime
            // (e.g., when type inference couldn't determine the exact struct type),
            // check if any struct has this field and use runtime lookup
            _ => {
                // Check if any struct definition has this field name
                let mut found_field = false;

                // Search in instantiated structs
                for (_, struct_info) in self.shared_ctx.struct_table.iter() {
                    if struct_info.fields.iter().any(|(name, _)| name == field) {
                        found_field = true;
                        break;
                    }
                }

                // Also search in parametric struct definitions
                if !found_field {
                    for (_, param_def) in self.shared_ctx.parametric_structs.iter() {
                        if param_def.def.fields.iter().any(|f| f.name == field) {
                            found_field = true;
                            break;
                        }
                    }
                }

                if found_field {
                    // Use runtime field lookup by name since different structs may have
                    // the same field name at different indices.
                    self.emit(Instr::GetFieldByName(field.to_string()));
                    // Return Any because we don't know the actual struct type at compile time.
                    // The actual field type depends on the runtime struct instance.
                    Ok(ValueType::Any)
                } else {
                    err(format!(
                        "Field access requires a struct type, got {:?}",
                        obj_ty
                    ))
                }
            }
        }
    }

    /// Whether dispatching `getproperty(obj, ::Symbol)` for a struct-typed
    /// receiver resolves to a *user-defined* method rather than the Base default
    /// (`getproperty(x, f::Symbol) = getfield(x, f)`). Used by
    /// [`Self::compile_field_access`] to decide whether `obj.field` must route
    /// through the override (Issue #8127).
    ///
    /// Only nominal struct receivers are considered: primitives, arrays, tuples,
    /// modules, etc. never carry a user `getproperty` override in practice, and
    /// gating on `JuliaType::Struct` keeps the common field-access fast path and
    /// avoids re-dispatching an imprecise (`Any`) receiver through the override.
    fn struct_type_has_user_getproperty_override(&self, obj_julia_type: &JuliaType) -> bool {
        if !matches!(obj_julia_type, JuliaType::Struct(_)) {
            return false;
        }
        for table_name in ["getproperty", "Base.getproperty"] {
            let Some(table) = self.method_tables.get(table_name) else {
                continue;
            };
            let resolved_global_index = table
                .dispatch(&[obj_julia_type.clone(), JuliaType::Symbol])
                .ok()
                .map(|sig| sig.global_index);
            // A user-defined method is one whose IR the program carries; Base /
            // prelude `getproperty` methods are absent from this map (the same
            // user-origin discriminator the inference seeding uses in
            // `core_compiler.rs`).
            if resolved_global_index.is_some_and(|global_index| {
                self.shared_ctx
                    .function_ir_by_global_index
                    .contains_key(&global_index)
            }) {
                return true;
            }
        }
        false
    }

    /// Compile `obj.field` as `getproperty(obj, :field)` so a user override
    /// intercepts the access (Issue #8127). The default `getproperty` falls back
    /// to `getfield`, so declared fields keep working through the same path.
    fn compile_getproperty_override_call(
        &mut self,
        object: &Expr,
        field: &str,
    ) -> CResult<ValueType> {
        let span = object.span();
        let args = [
            object.clone(),
            Expr::Literal(Literal::Symbol(field.to_string()), span),
        ];
        self.compile_call("getproperty", &args, &[], &[], &[])
    }
}

/// True for abstract numeric field types whose `ValueType` mapping lossily
/// collapses to a *concrete* representation (`Real`/`Number`/`AbstractFloat` ->
/// `F64`, `Integer`/`Signed`/`Unsigned` -> `I64`).
///
/// A struct field declared with such a type — including a struct type parameter
/// bound to an abstract type, e.g. `struct Foo{T} x::T end; Foo{Real}(1)` —
/// must keep the supplied value's *original* concrete type/width. Julia's
/// default constructor only inserts `convert(fieldtype, x)`, which is a no-op
/// when `x isa fieldtype`, so `Foo{Real}(1).x` is the `Int64` `1`, not the
/// `Float64` `1.0`. Coercing through the lossy `ValueType` would clobber that
/// (Issue #5060).
pub(in crate::compile::expr) fn field_type_is_abstract_numeric(
    ty: &crate::types::JuliaType,
) -> bool {
    use crate::types::JuliaType;
    matches!(
        ty,
        JuliaType::Number
            | JuliaType::Real
            | JuliaType::AbstractFloat
            | JuliaType::Integer
            | JuliaType::Signed
            | JuliaType::Unsigned
    )
}

/// True for concrete primitive field types whose declared width is *not*
/// faithfully represented by their `ValueType` mapping, so a compile-time
/// `compile_expr_as` coercion would silently clobber the value's width.
///
/// `julia_type_to_value_type` collapses every signed/unsigned integer to
/// `ValueType::I64` and `Float16`/`Float32` are distinct only as `F16`/`F32`.
/// For these fields the constructor argument must be left untouched at compile
/// time and coerced precisely at runtime in the `NewStruct` step (Issue #4990).
///
/// `Int64`, `Float64`, and `Bool` round-trip losslessly through `ValueType`, so
/// they keep using the existing compile-time coercion path.
pub(in crate::compile::expr) fn field_type_needs_runtime_coercion(
    ty: &crate::types::JuliaType,
) -> bool {
    use crate::types::JuliaType;
    matches!(
        ty,
        JuliaType::Int8
            | JuliaType::Int16
            | JuliaType::Int32
            | JuliaType::Int128
            | JuliaType::UInt8
            | JuliaType::UInt16
            | JuliaType::UInt32
            | JuliaType::UInt64
            | JuliaType::UInt128
            | JuliaType::Float16
            | JuliaType::Float32
    )
}

#[cfg(test)]
mod tests {
    use super::{field_type_is_abstract_numeric, field_type_needs_runtime_coercion};
    use crate::types::JuliaType;

    #[test]
    fn abstract_numeric_field_types_skip_compile_time_coercion_issue_5060() {
        // Abstract numeric bounds must preserve the supplied concrete value, so
        // they are treated like `Any` at the struct constructor (no coercion).
        for ty in [
            JuliaType::Number,
            JuliaType::Real,
            JuliaType::AbstractFloat,
            JuliaType::Integer,
            JuliaType::Signed,
            JuliaType::Unsigned,
        ] {
            assert!(
                field_type_is_abstract_numeric(&ty),
                "{ty:?} should be treated as an abstract numeric field type"
            );
        }
    }

    #[test]
    fn concrete_field_types_are_not_abstract_numeric_issue_5060() {
        // Concrete fields still go through their normal coercion path; they must
        // not be misclassified as abstract.
        for ty in [
            JuliaType::Int64,
            JuliaType::Int32,
            JuliaType::UInt64,
            JuliaType::Float64,
            JuliaType::Float32,
            JuliaType::Bool,
            JuliaType::Any,
        ] {
            assert!(
                !field_type_is_abstract_numeric(&ty),
                "{ty:?} must not be treated as an abstract numeric field type"
            );
        }
    }

    #[test]
    fn abstract_and_runtime_coercion_classifications_are_disjoint_issue_5060() {
        // A field type cannot simultaneously be "preserve as-is" (abstract) and
        // "coerce precisely at runtime" (narrow concrete width). Keeping these
        // disjoint guarantees the constructor branch ordering is unambiguous.
        for ty in [
            JuliaType::Number,
            JuliaType::Real,
            JuliaType::AbstractFloat,
            JuliaType::Integer,
            JuliaType::Signed,
            JuliaType::Unsigned,
            JuliaType::Int8,
            JuliaType::Int32,
            JuliaType::UInt64,
            JuliaType::Float16,
            JuliaType::Float32,
            JuliaType::Int64,
            JuliaType::Float64,
        ] {
            assert!(
                !(field_type_is_abstract_numeric(&ty) && field_type_needs_runtime_coercion(&ty)),
                "{ty:?} classified as both abstract-numeric and runtime-coercion"
            );
        }
    }
}
