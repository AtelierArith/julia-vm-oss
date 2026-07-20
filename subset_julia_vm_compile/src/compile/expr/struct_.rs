//! Struct compilation (constructors and field access).

use crate::builtins::BuiltinId;
use crate::bytecode::value::is_array_wrapper_struct_name;
use crate::bytecode::{ArrayElementType, Instr, ValueType};
use crate::ir::core::{Expr, Literal};
use crate::types::JuliaType;

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
const GLOBAL_REF_FIELD_BINDING_INDEX: usize = 2;

fn core_builtin_type_constant(field: &str) -> Option<&'static str> {
    match field {
        "Binding" => Some("Core.Binding"),
        _ => None,
    }
}

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
                // Field types whose precise Julia conversion cannot be represented
                // by a `ValueType` coercion: narrow/unsigned integers, non-Float64
                // floats, and concrete Complex instantiations. Compile the argument
                // as-is and let the runtime `NewStruct` step apply the precise
                // `convert(fieldtype, x)` (Issues #4990 / #9381).
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

    /// Inline the synthetic default inner constructor without embedding a
    /// compiler-generated function index in the caller (Issue #11147).
    ///
    /// Upstream's `convert-for-type-decl` first evaluates every call argument,
    /// then converts fields from left to right with
    /// `value isa fieldtype ? value : convert(fieldtype, value)`, and allocates
    /// only after all conversions succeed. Keeping that exact shape here makes
    /// conversion failures catchable, honors user `Base.convert` methods, and
    /// preserves persistent-REPL live append for newly compiled callers.
    ///
    /// Returns `None` before emitting bytecode when the field target cannot be
    /// represented as a stable runtime type object; the caller can then use the
    /// ordinary synthetic-method dispatch path.
    pub(in super::super) fn try_compile_synthetic_default_inner_inline(
        &mut self,
        struct_info: &StructInfo,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        if args.len() != struct_info.fields.len() {
            return Ok(None);
        }

        let Some(precise_field_types) = self
            .shared_ctx
            .field_julia_types_by_type_id(struct_info.type_id)
            .map(<[_]>::to_vec)
        else {
            return Ok(None);
        };
        if precise_field_types.len() != struct_info.fields.len() {
            return Ok(None);
        }

        // Preflight every target before compiling an argument. Falling back
        // after emitting only part of a call would corrupt the caller's stack.
        let mut field_targets = Vec::with_capacity(precise_field_types.len());
        for ((arg, precise), (_, value_type)) in args
            .iter()
            .zip(precise_field_types.iter())
            .zip(struct_info.fields.iter())
        {
            let actual = self.infer_julia_type(arg);
            if matches!(precise, JuliaType::Any)
                || (!matches!(actual, JuliaType::Any) && actual.is_subtype_of(precise))
            {
                field_targets.push(None);
                continue;
            }
            if synthetic_field_type_needs_runtime_binding(precise) {
                return Ok(None);
            }
            let target = match value_type {
                ValueType::Struct(type_id) => {
                    // A lossy ValueType lookup can land on an unrelated
                    // same-leaf struct when the declared field is actually a
                    // primitive/type alias. Inline only when the precise
                    // JuliaType names the exact same nominal owner; otherwise
                    // let ordinary constructor dispatch resolve the alias.
                    let JuliaType::Struct(precise_name) = precise else {
                        return Ok(None);
                    };
                    let Some(target) = self.shared_ctx.get_struct_name(*type_id) else {
                        return Ok(None);
                    };
                    if self.resolve_struct_name(precise_name).as_deref() != Some(target.as_str()) {
                        return Ok(None);
                    }
                    target
                }
                _ if synthetic_field_type_contains_nominal_struct(precise) => return Ok(None),
                _ => precise.name().into_owned(),
            };
            field_targets.push(Some(target));
        }

        // Julia evaluates all arguments before entering the inner constructor.
        // Save them first so conversion side effects cannot run between argument
        // evaluations.
        let mut arg_temps = Vec::with_capacity(args.len());
        for arg in args {
            let temp = self.new_temp("synthetic_ctor_arg");
            self.compile_expr(arg)?;
            self.emit(Instr::StoreAny(temp.clone()));
            arg_temps.push(temp);
        }

        for (temp, target) in arg_temps.iter().zip(field_targets.iter()) {
            let Some(target) = target else {
                self.emit(Instr::LoadAny(temp.clone()));
                continue;
            };

            self.emit(Instr::LoadAny(temp.clone()));
            self.emit(Instr::PushDataType(target.clone()));
            self.emit(Instr::CallBuiltin(BuiltinId::Isa, 2));
            let convert_jump = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));

            self.emit(Instr::LoadAny(temp.clone()));
            let converted_jump = self.here();
            self.emit(Instr::Jump(usize::MAX));

            let convert_start = self.here();
            self.patch_jump(convert_jump, convert_start);
            self.emit(Instr::PushDataType(target.clone()));
            self.emit(Instr::LoadAny(temp.clone()));
            self.emit(Instr::CallBuiltin(BuiltinId::Convert, 2));

            let converted = self.here();
            self.patch_jump(converted_jump, converted);
        }

        self.emit(Instr::NewStruct(struct_info.type_id, args.len()));
        Ok(Some(ValueType::Struct(struct_info.type_id)))
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
                // `compile_expr_as` intentionally permits direct coercion into
                // the losslessly represented numeric field types below. A
                // synthetic Julia constructor, however, must honor `convert`
                // (including user methods and catchable `InexactError`) whenever
                // the source is not already a subtype (Issue #11147). Reject
                // only that raw-allocation shortcut; collection fields retain
                // their existing type-preserving fast path (Issue #9188).
                let numeric_runtime_convert_target = matches!(
                    field_ty,
                    ValueType::I64 | ValueType::F64 | ValueType::BigInt | ValueType::BigFloat
                );
                if numeric_runtime_convert_target {
                    let actual_julia_type = self.infer_julia_type(arg);
                    if precise.is_some_and(|target| {
                        !matches!(&actual_julia_type, JuliaType::Any)
                            && !actual_julia_type.is_subtype_of(target)
                    }) {
                        return false;
                    }
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
        if let Some(owned) = self.module_path_in_current_scope(path) {
            return Some(owned);
        }
        if self.imported_binding_root(path).is_some() {
            return None;
        }
        // Canonicalize via the shared alias resolver first (resolving an
        // alias-rooted path like `AA.B.C` with `const AA = A` to `A.B.C`) BEFORE
        // the known-module lookup. Resolving first avoids matching a propagated
        // alias spelling (`AA.B.C` can appear in `module_exports`) that
        // `compile_module_function_ref` cannot then resolve.
        self.resolve_visible_module_path(path)
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
                    if self.is_renamed_only_module_root(name) {
                        return false;
                    }
                    if self.explicit_lexical_owner_active(name) {
                        return true;
                    }
                    if self.captured_vars.contains(name.as_str()) {
                        return true;
                    }
                    // A statically registered const/import alias remains a
                    // module-path root (`const MA = Mod1; MA.S`). A mere local
                    // or parameter inferred as `Module` still shadows any
                    // same-named static module: its concrete module identity is
                    // a runtime value (`f(m::Module) = m.x`, Issues
                    // #7245/#8114/#11176).
                    let has_local = self.locals.contains_key(name.as_str());
                    // Whole-block inference pre-seeds imported names in
                    // `locals`; that metadata is not a lexical shadow. Only an
                    // initialized local/parameter or capture may hide the
                    // source-ordered runtime import (Issues #11176/#11216).
                    if has_local
                        && self.local_scope_depth > 0
                        && self.module_alias_states.contains_key(name.as_str())
                    {
                        return true;
                    }
                    if self.imported_bindings.contains(name.as_str())
                        && !self.initialized_locals.contains(name.as_str())
                    {
                        return false;
                    }
                    if !self.module_alias_states.contains_key(name.as_str())
                        && self.module_aliases.contains_key(name.as_str())
                        && !self.initialized_locals.contains(name.as_str())
                    {
                        return false;
                    }
                    if has_local && (self.strict_undefined_check || self.local_scope_depth > 0) {
                        return true;
                    }
                    if self.module_aliases.contains_key(name.as_str()) {
                        return false;
                    }
                    return has_local && self.initialized_locals.contains(name.as_str());
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
            let root = module_path.split('.').next().unwrap_or(&module_path);
            if !self.module_path_root_shadowed_by_local(object)
                && self.imported_binding_root(root).is_none()
                && self.resolve_visible_module_path(root).is_none()
                && (self.is_known_module_path(&module_path) || self.is_known_module_path(root))
            {
                return Ok(self.emit_unbound_module_name(root));
            }
            if module_path == "Core" {
                if let Some(type_name) = core_builtin_type_constant(field) {
                    self.emit(Instr::PushDataType(type_name.to_string()));
                    return Ok(ValueType::DataType);
                }
            }

            // Check if this is Base.MathConstants constant access
            if module_path == "Base.MathConstants" {
                if let Some(ty) = self.emit_builtin_irrational_singleton(field) {
                    return Ok(ty);
                }
                if field == "e" {
                    if let Some(ty) = self.emit_builtin_irrational_singleton("\u{212F}") {
                        return Ok(ty);
                    }
                }
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
                if let Some(ty) = self.emit_builtin_irrational_singleton(field) {
                    return Ok(ty);
                }
                if let Some(value) = get_base_exported_constant_value(field) {
                    self.emit(Instr::PushF64(value));
                    return Ok(ValueType::F64);
                }
            }

            // Handle other Base submodules or module function refs. Base preload
            // submodules can be represented as top-level IR modules, so
            // canonicalize before the legacy Base-submodule fallback (Issue #8269).
            let canonical_module_path = self.canonical_module_path(&module_path);
            if canonical_module_path != module_path || module_path.starts_with("Base.") {
                return self.compile_resolved_module_function_ref(&canonical_module_path, field);
            }
            if is_stdlib_module(&module_path)
                && self.resolve_visible_module_path(&module_path).is_some()
            {
                return self.compile_resolved_module_function_ref(&canonical_module_path, field);
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
            if module_name == "Core" {
                if let Some(type_name) = core_builtin_type_constant(field) {
                    self.emit(Instr::PushDataType(type_name.to_string()));
                    return Ok(ValueType::DataType);
                }
            }

            let local_ty = self.locals.get(module_name.as_str()).cloned();
            // A local binding (function parameter / local variable) shadows a
            // same-named module in scope, matching Julia's scoping rules. Without
            // this, a method like `f(D::Diagonal) = D.diag[i]` defined inside a
            // user module literally named `D` mis-resolves the field access
            // `D.diag` as the module-qualified call `D.diag(...)` and fails with
            // "Module D has no function named diag" (Issue #7245). The lone
            // exception is a local that actually holds a module value, which is
            // still a module access.
            let shadowed_by_local = self.module_path_root_shadowed_by_local(object);
            let owned_module_path = self.module_path_in_current_scope(module_name);
            if !shadowed_by_local
                && owned_module_path.is_none()
                && self.imported_binding_root(module_name).is_some()
            {
                self.emit_load_imported_binding(module_name);
                self.emit(Instr::GetFieldByName(field.to_string()));
                return Ok(ValueType::Any);
            }
            let resolved_visible_module = (!shadowed_by_local)
                .then(|| self.resolve_visible_module_path(module_name))
                .flatten();
            let is_module_value = !shadowed_by_local
                && (matches!(local_ty, Some(ValueType::Module))
                    || resolved_visible_module.is_some());

            if is_module_value {
                let resolved_module = resolved_visible_module
                    .unwrap_or_else(|| self.resolve_module_alias_path(module_name));
                let resolved_module = self.canonical_module_path(&resolved_module);

                // Handle Base module constants (pi, e, Inf, NaN, etc.)
                // These are exported from Base.MathConstants but accessible as Base.pi
                if resolved_module == "Base" {
                    if let Some(ty) = self.emit_builtin_irrational_singleton(field) {
                        return Ok(ty);
                    }
                    if let Some(value) = get_math_constant_value(field) {
                        self.emit(Instr::PushF64(value));
                        return Ok(ValueType::F64);
                    }
                }
                if resolved_module == "Sys" && field == "WORD_SIZE" {
                    self.emit(Instr::PushI64(i64::from(usize::BITS)));
                    return Ok(ValueType::I64);
                }

                return self.compile_resolved_module_function_ref(&resolved_module, field);
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

                if let Some((_, name, struct_info)) =
                    self.shared_ctx.struct_table.resolve_type_id(type_id)
                {
                    struct_name = name.clone();
                    for (idx, (field_name, field_ty)) in struct_info.fields.iter().enumerate() {
                        if field_name == field {
                            result = Some((idx, field_ty.clone()));
                            break;
                        }
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
                            // Issue #10319: a statically-known-bogus field on a
                            // struct whose type IS resolved (e.g. `Foo().nope`)
                            // must defer to the same catchable runtime FieldError
                            // upstream Julia raises, not abort compilation of the
                            // whole program. `Instr::GetFieldByName` already
                            // raises `VmError::FieldError` for exactly this case
                            // on the dynamic (`ValueType::Any`) path below; route
                            // the statically-typed receiver through the identical
                            // instruction so both paths share one error site.
                            self.emit(Instr::GetFieldByName(field.to_string()));
                            Ok(ValueType::Any)
                        }
                    }
                }
            }
            ValueType::Any => {
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
                _ => {
                    // Issue #10319: a statically-known-bogus field on a
                    // DataType receiver (e.g. `Int.bogus`) must defer to the
                    // same catchable runtime FieldError upstream Julia
                    // raises, not abort compilation of the whole program.
                    // `Instr::GetFieldByName` already handles arbitrary
                    // `Value::DataType` field lookups generically (recognized
                    // fields + FieldError fallback) for the dynamic
                    // (`ValueType::Any`) path; route the statically-typed
                    // receiver through the identical instruction so both
                    // paths share one error site.
                    self.emit(Instr::GetFieldByName(field.to_string()));
                    Ok(ValueType::Any)
                }
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
                        Ok(ValueType::ArrayOf(ArrayElementType::Any, Some(1))) // Vector{Any}
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
            // GlobalRef type has special fields: mod, name, and binding.
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
                "binding" => {
                    self.emit(Instr::GetGlobalRefField(GLOBAL_REF_FIELD_BINDING_INDEX));
                    Ok(ValueType::Any)
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
                for struct_info in self.shared_ctx.struct_table.values() {
                    if struct_info.fields.iter().any(|(name, _)| name == field) {
                        found_field = true;
                        break;
                    }
                }

                // Also search in parametric struct definitions
                if !found_field {
                    for param_def in self.shared_ctx.parametric_structs.values() {
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
                    // Issue #10319: a statically-known primitive/non-struct
                    // receiver (Int64, Float64, Bool, Tuple, ...) with a field
                    // name that matches no struct anywhere in the program is a
                    // statically-known-bogus field access, exactly like
                    // `Foo().nope` / `Int.bogus` above. Upstream Julia always
                    // defers this to a catchable runtime FieldError rather than
                    // rejecting the whole program at compile time; route it
                    // through the same `GetFieldByName` instruction so the
                    // runtime raises the matching error.
                    self.emit(Instr::GetFieldByName(field.to_string()));
                    Ok(ValueType::Any)
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

fn synthetic_field_type_needs_runtime_binding(field_type: &JuliaType) -> bool {
    match field_type {
        JuliaType::TypeVar(..)
        | JuliaType::RuntimeTypeVar { .. }
        | JuliaType::RuntimeParametric { .. }
        | JuliaType::UnionAll { .. }
        | JuliaType::RuntimeUnionAll { .. } => true,
        JuliaType::VectorOf(element)
        | JuliaType::MatrixOf(element)
        | JuliaType::TypeOf(element) => synthetic_field_type_needs_runtime_binding(element),
        JuliaType::TupleOf(elements) | JuliaType::Union(elements) => elements
            .iter()
            .any(synthetic_field_type_needs_runtime_binding),
        _ => false,
    }
}

fn synthetic_field_type_contains_nominal_struct(field_type: &JuliaType) -> bool {
    match field_type {
        JuliaType::Struct(_) => true,
        JuliaType::VectorOf(element)
        | JuliaType::MatrixOf(element)
        | JuliaType::TypeOf(element) => synthetic_field_type_contains_nominal_struct(element),
        JuliaType::TupleOf(elements) | JuliaType::Union(elements) => elements
            .iter()
            .any(synthetic_field_type_contains_nominal_struct),
        _ => false,
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

/// True for field types whose declared conversion is *not* faithfully represented
/// by their `ValueType` mapping, so a compile-time `compile_expr_as` coercion
/// would silently clobber the value or reject an imprecise-but-convertible value.
///
/// `julia_type_to_value_type` collapses every signed/unsigned integer to
/// `ValueType::I64` and `Float16`/`Float32` are distinct only as `F16`/`F32`.
/// Concrete `Complex{T}` fields also need the runtime path because generic
/// Complex arithmetic can infer an imprecise Complex element type while the
/// actual runtime value is still convertible to the declared field type.
/// For these fields the constructor argument must be left untouched at compile
/// time and coerced precisely at runtime in the `NewStruct` step.
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
    ) || matches!(ty, JuliaType::Struct(name) if is_concrete_complex_struct_name(name))
}

fn is_concrete_complex_struct_name(name: &str) -> bool {
    matches!(name, "ComplexF64" | "ComplexF32")
        || (name.starts_with("Complex{") && name.ends_with('}'))
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::compile::context::StructRegistry;

    use super::{
        field_type_is_abstract_numeric, field_type_needs_runtime_coercion, ArrayElementType,
        CoreCompiler, Expr, ValueType,
    };
    use crate::compile::context::SharedCompileContext;
    use crate::span::Span;
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

    // ── Issue #9673: Expr field type drift between compiler emit and inference ──
    //
    // Root cause (Issue #9557): `Expr.args` had two compiler-facing type
    // sources. `compile_field_access` (this file) answered `Vector{Any}` while
    // `infer_expr_type` (compile/expr/infer/mod.rs) still answered the legacy
    // `ValueType::Array`. `push!` codegen consults inference for a non-variable
    // receiver (`BuiltinOp::Push` in compile/expr/builtin.rs), so
    // `push!(expr.args, 7)` silently routed through Float64 array coercion
    // (`PushI64(7); ToF64; ArrayPush`). The tests below probe both type
    // sources directly against the same `Expr::FieldAccess` AST node so a
    // future drift between them is caught at the unit-test level instead of
    // waiting for a `push!`-shaped symptom.

    /// Minimal owned backing storage for a `CoreCompiler<'_>`, used only to
    /// probe `compile_field_access` / `infer_expr_type` in isolation without
    /// running the full parse/lower/compile pipeline.
    struct FieldTypeProbeFixture {
        method_tables: HashMap<String, crate::compile::MethodTable>,
        module_functions: HashMap<String, HashSet<String>>,
        module_exports: HashMap<String, HashSet<String>>,
        imported_functions: HashSet<String>,
        usings: HashSet<String>,
        abstract_type_names: HashSet<String>,
        module_constants: HashMap<String, HashSet<String>>,
        shared_ctx: SharedCompileContext,
    }

    impl FieldTypeProbeFixture {
        fn new() -> Self {
            Self {
                method_tables: HashMap::new(),
                module_functions: HashMap::new(),
                module_exports: HashMap::new(),
                imported_functions: HashSet::new(),
                usings: HashSet::new(),
                abstract_type_names: HashSet::new(),
                module_constants: HashMap::new(),
                shared_ctx: SharedCompileContext::new(
                    StructRegistry::new(),
                    Vec::new(),
                    HashMap::new(),
                    HashMap::new(),
                    Vec::new(),
                    0,
                ),
            }
        }

        fn compiler(&mut self) -> CoreCompiler<'_> {
            CoreCompiler::new(
                &self.method_tables,
                &self.module_functions,
                &self.module_exports,
                &self.imported_functions,
                &self.usings,
                Vec::new(),
                &mut self.shared_ctx,
                &self.abstract_type_names,
                &self.module_constants,
            )
        }
    }

    /// Returns `(compile_field_access type, infer_expr_type type)` for
    /// `obj.field`, where `obj` is a synthetic local pre-typed as
    /// `receiver_ty`. Both code paths run against the identical
    /// `Expr::FieldAccess` AST node, so any difference is a genuine drift
    /// between the two compiler-facing type sources (Issue #9673 root cause).
    fn field_access_types(receiver_ty: ValueType, field: &str) -> (ValueType, ValueType) {
        let mut fixture = FieldTypeProbeFixture::new();
        let mut compiler = fixture.compiler();
        let span = Span::new(0, 0, 0, 0, 0, 0);
        compiler.locals.insert("obj".to_string(), receiver_ty);
        let object = Expr::Var("obj".to_string().into(), span);
        let compile_ty = compiler
            .compile_field_access(&object, field)
            .unwrap_or_else(|e| {
                panic!("compile_field_access(.{field}) failed unexpectedly: {e:?}")
            });
        let field_access = Expr::FieldAccess {
            object: Box::new(object),
            field: field.to_string().into(),
            span,
        };
        let infer_ty = compiler.infer_expr_type(&field_access);
        (compile_ty, infer_ty)
    }

    #[test]
    fn expr_head_field_access_compile_and_infer_agree_9673() {
        let (compile_ty, infer_ty) = field_access_types(ValueType::Expr, "head");
        assert_eq!(compile_ty, ValueType::Symbol);
        assert_eq!(
            infer_ty, compile_ty,
            "infer_expr_type must mirror compile_field_access for Expr.head (Issue #9673)"
        );
    }

    #[test]
    fn expr_args_field_access_compile_and_infer_agree_9673() {
        // The exact #9557 regression: infer_expr_type used to answer the
        // legacy `ValueType::Array` for this field while compile_field_access
        // already answered `Vector{Any}`, so `push!(expr.args, 7)` routed
        // through Float64 array coercion in BuiltinOp::Push.
        let (compile_ty, infer_ty) = field_access_types(ValueType::Expr, "args");
        assert_eq!(
            compile_ty,
            ValueType::ArrayOf(ArrayElementType::Any, Some(1))
        );
        assert_eq!(
            infer_ty, compile_ty,
            "infer_expr_type must mirror compile_field_access for Expr.args (Issue #9673 / #9557)"
        );
    }

    #[test]
    fn builtin_struct_field_types_stay_compile_infer_consistent_9673() {
        // Table-driven prevention (Issue #9673): every "special" ValueType
        // field-access branch in `compile_field_access` (this file) must be
        // safely mirrored by `infer_expr_type` (compile/expr/infer/mod.rs).
        // New fields added to either side are automatically covered by
        // extending this list; the two sources cannot silently drift again.
        //
        // Rule: an EXACT match is required whenever the compile-path type is
        // Array/ArrayOf — those feed element-type-keyed numeric coercion
        // decisions (push!/pushfirst!, see `BuiltinOp::Push` in builtin.rs)
        // where a wrong `infer_expr_type` answer silently corrupts the pushed
        // value (Issue #9557). A scalar compile-path type may safely widen to
        // `Any` under inference — that only forces the generic/dynamic codegen
        // path (slower, never wrong), since no coercion decision keys off it.
        let cases: &[(ValueType, &str)] = &[
            (ValueType::Expr, "head"),
            (ValueType::Expr, "args"),
            (ValueType::LineNumberNode, "line"),
            (ValueType::LineNumberNode, "file"),
            (ValueType::QuoteNode, "value"),
            (ValueType::GlobalRef, "mod"),
            (ValueType::GlobalRef, "name"),
            (ValueType::GlobalRef, "binding"),
            (ValueType::DataType, "parameters"),
            (ValueType::DataType, "var"),
            (ValueType::DataType, "body"),
            (ValueType::DataType, "name"),
            (ValueType::DataType, "lb"),
            (ValueType::DataType, "ub"),
        ];

        for (receiver_ty, field) in cases.iter().cloned() {
            let (compile_ty, infer_ty) = field_access_types(receiver_ty.clone(), field);
            let is_array_like = matches!(compile_ty, ValueType::Array | ValueType::ArrayOf(_, _));
            let drift_is_safe =
                infer_ty == compile_ty || (!is_array_like && infer_ty == ValueType::Any);
            assert!(
                drift_is_safe,
                "Issue #9673: {receiver_ty:?}.{field} compile_field_access returns \
                 {compile_ty:?} but infer_expr_type returns {infer_ty:?}. \
                 Array/ArrayOf-typed fields must match exactly (Issue #9557 push! \
                 coercion trap); non-array fields may only widen to Any."
            );
        }
    }
}
