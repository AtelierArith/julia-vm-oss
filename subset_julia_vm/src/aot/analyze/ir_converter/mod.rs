//! IR conversion from Core IR to AoT IR.
//!
//! The `IrConverter` translates the Core IR representation into AoT IR,
//! performing type annotation and specialization based on inference results.

use super::super::abi::AotAbiValue;
use super::super::inference::{TypeEnv, TypeInferenceEngine, TypedFunction, TypedProgram};
use super::super::ir::{
    AotBinOp, AotBuiltinOp, AotEnum, AotExpr, AotFunction, AotGlobal, AotInlinePolicy, AotProgram,
    AotStmt, AotStruct, AotUnaryOp,
};
use super::super::types::StaticType;
use super::super::{AotError, AotResult, UnsupportedInstructionDiagnostic};
use crate::ir::core::{Block, EnumDef, Expr, Function, Literal, Program, Stmt, StructDef};
use crate::types::{JuliaType, TypeExpr};
use std::collections::{HashMap, HashSet};

pub(crate) struct IrConverter<'a> {
    /// Type information from inference
    typed: &'a TypedProgram,
    /// Type inference engine for expression type inference
    pub(crate) engine: TypeInferenceEngine,
    /// Set of declared local variables in current scope
    pub(crate) declared_locals: HashSet<String>,
    /// Reference to program functions for lambda lookup
    functions: HashMap<String, &'a Function>,
    /// Function names whose source method set includes an explicit `::Any`
    /// parameter. Only these need occurrence-based overload conversion for
    /// Issue #7158; internal inference may add Any-shaped specializations for
    /// ordinary functions that should keep the legacy first-signature behavior.
    generic_any_function_names: HashSet<String>,
    /// Number of converted methods per Julia function name for generic `::Any`
    /// overload sets. TypedProgram stores overloads grouped by name, so those
    /// methods must pick the matching occurrence instead of reusing the first
    /// signature for every method (Issue #7158).
    function_occurrences: HashMap<String, usize>,
    /// Current function's return type (for type coercion in return statements)
    current_return_type: Option<StaticType>,
    /// User-declared abstract type names with their parent and type parameter
    /// names, used to resolve static subtype (`<:`) relations (Issue #7037).
    abstract_types: Vec<(String, Option<String>, Vec<String>)>,
}

mod expr;
mod helpers;
mod stmt;

impl<'a> IrConverter<'a> {
    /// Create a new IR converter
    pub(crate) fn new(typed: &'a TypedProgram, program: &'a Program) -> Self {
        // Create a new type inference engine with struct info from typed program
        let mut engine = TypeInferenceEngine::new();
        // Copy struct info so that constructor calls can be inferred correctly
        engine.structs = typed.structs.clone();
        for struct_def in &program.structs {
            if !engine.structs.contains_key(&struct_def.name) {
                if let Ok(info) = engine.analyze_struct(struct_def) {
                    engine.structs.insert(struct_def.name.clone(), info);
                }
            }
        }
        // Copy globals from typed program for tuple indexing
        engine.env = typed.globals.clone();
        let mut enum_defs = Vec::new();
        crate::aot::inference::collect_enum_defs_in_block(&program.main, &mut enum_defs);
        for enum_def in &program.enums {
            enum_defs.push(enum_def);
        }
        for enum_def in enum_defs {
            for member in &enum_def.members {
                engine
                    .enum_members
                    .insert(member.name.clone(), StaticType::I32);
                engine
                    .enum_member_values
                    .insert(member.name.clone(), member.value as i32);
                engine.env.insert(member.name.clone(), StaticType::I32);
            }
        }
        for funcs in typed.functions.values() {
            for func in funcs {
                engine.register_builtin(
                    &func.signature.name,
                    func.signature.param_types.clone(),
                    func.signature.return_type.clone(),
                );
            }
        }

        // Build function lookup map for lambda conversion
        let functions: HashMap<String, &'a Function> = program
            .functions
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();
        let generic_any_function_names = program
            .functions
            .iter()
            .filter(|func| {
                func.params.iter().any(|param| {
                    param
                        .type_annotation
                        .as_ref()
                        .is_some_and(|ty| matches!(ty, JuliaType::Any))
                })
            })
            .map(|func| func.name.clone())
            .collect();

        // Collect user-declared abstract types (top-level and within modules)
        // so static `<:` relations against abstract supertypes resolve
        // (Issue #7037).
        let mut abstract_types: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
        let mut collect_abstracts = |defs: &[crate::ir::core::AbstractTypeDef]| {
            for a in defs {
                abstract_types.push((
                    a.name.clone(),
                    a.parent.clone(),
                    a.type_params.iter().map(|p| p.name.clone()).collect(),
                ));
            }
        };
        collect_abstracts(&program.abstract_types);
        for module in &program.modules {
            collect_abstracts(&module.abstract_types);
        }

        Self {
            typed,
            engine,
            declared_locals: HashSet::new(),
            functions,
            generic_any_function_names,
            function_occurrences: HashMap::new(),
            current_return_type: None,
            abstract_types,
        }
    }

    /// Check if a function name is a lifted lambda
    pub(crate) fn is_lambda_function(&self, name: &str) -> bool {
        name.starts_with("__lambda_")
    }

    /// Get a lambda function by name
    fn get_lambda_function(&self, name: &str) -> Option<&'a Function> {
        if self.is_lambda_function(name) {
            self.functions.get(name).copied()
        } else {
            None
        }
    }

    fn global_static_initializer_supported(ty: &StaticType) -> bool {
        matches!(
            ty,
            StaticType::I64
                | StaticType::I128
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U128
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
                | StaticType::F64
                | StaticType::F32
                | StaticType::F16
                | StaticType::Bool
                | StaticType::Char
                | StaticType::Nothing
                | StaticType::Missing
        )
    }

    /// Struct names that are defined in the AoT prelude and should be skipped
    /// during conversion to avoid duplicate definitions (Issue #3410).
    const PRELUDE_STRUCT_NAMES: &'static [&'static str] = &[
        "ErrorException",
        "LinRange",
        "StepRangeLen",
        "OneTo",
        "Broadcasted",
        "Rational",
    ];

    pub(crate) fn convert_program(&mut self, program: &Program) -> AotResult<AotProgram> {
        let mut aot_program = AotProgram::new();
        let used_parametric_structs = Self::collect_used_parametric_structs(program);

        // Convert struct definitions, deduplicating by name (Issue #3410).
        // The prelude already defines ErrorException; Base may also emit it.
        let mut seen_structs: HashSet<String> = HashSet::new();
        for struct_def in &program.structs {
            // Skip structs that are already defined in the prelude
            if Self::PRELUDE_STRUCT_NAMES.contains(&struct_def.name.as_str()) {
                continue;
            }
            // Skip duplicate struct definitions
            if !seen_structs.insert(struct_def.name.clone()) {
                continue;
            }
            // Parametric structs from Base may be reachable without being used
            // by the user program. Emit only those actually constructed so
            // unrelated Base machinery cannot poison AoT codegen (Issue #7251).
            if struct_def.is_parametric()
                && struct_def.name != "Complex"
                && !used_parametric_structs.contains(&struct_def.name)
            {
                continue;
            }
            let aot_struct = self.convert_struct(struct_def)?;
            aot_program.add_struct(aot_struct);
        }

        // Convert enum definitions. `@enum` at top level lowers to a
        // `Stmt::EnumDef` in the main block rather than `program.enums`, so
        // collect those too — otherwise the enum (and its member constants) is
        // never emitted and references type as `Any` (Issue #7050).
        let mut seen_enums: HashSet<String> = HashSet::new();
        for enum_def in &program.enums {
            if seen_enums.insert(enum_def.name.clone()) {
                aot_program.add_enum(Self::convert_enum(enum_def));
            }
        }
        let mut main_enum_defs = Vec::new();
        crate::aot::inference::collect_enum_defs_in_block(&program.main, &mut main_enum_defs);
        for enum_def in main_enum_defs {
            if seen_enums.insert(enum_def.name.clone()) {
                aot_program.add_enum(Self::convert_enum(enum_def));
            }
        }

        // Convert functions, excluding base library functions that are handled by AoT builtins
        let mut seen_function_signatures: HashSet<String> = HashSet::new();
        for func in &program.functions {
            // Skip base library operator/convert functions - these are handled as AoT builtins
            if Self::is_aot_builtin_function(&func.name) && func.is_base_extension {
                continue;
            }
            // Skip the pure-Julia Base bodies of string functions that AoT
            // intercepts as builtins at the call site (Issue #7058). Their Base
            // implementations pull in parametric/iterator machinery
            // (`HasShape{1}`, …) that AoT cannot lower; the call site routes to
            // the builtin instead.
            if crate::aot::call_graph::is_intercepted_string_builtin(&func.name) {
                continue;
            }
            let signature = Self::function_redefinition_key(func);
            if !seen_function_signatures.insert(signature) {
                return Err(AotError::UnsupportedInstruction(
                    UnsupportedInstructionDiagnostic::new(format!(
                        "AoT codegen does not support redefining function `{}` with the same signature (Issue #7061)",
                        func.name
                    ))
                    .with_span(func.span)
                    .with_workaround(
                        "keep a single method per concrete signature for AoT, or run world-age-dependent redefinition code on the VM",
                    ),
                ));
            }
            let aot_func = self.convert_function(func)?;
            aot_program.add_function(aot_func);
        }

        // Convert main block statements to globals and main execution
        self.declared_locals.clear();
        for stmt in &program.main.stmts {
            match stmt {
                Stmt::Assign { var, value, span } => {
                    // Check if this is a global variable declaration
                    if !self.declared_locals.contains(var) {
                        let ty = self.engine.infer_expr_type(value);
                        if !Self::global_static_initializer_supported(&ty) {
                            return Err(AotError::UnsupportedInstruction(
                                UnsupportedInstructionDiagnostic::new(format!(
                                    "top-level global `{}` of type `{}` cannot be emitted as a const Rust static initializer",
                                    var,
                                    ty.julia_type_name()
                                ))
                                .with_span(*span)
                                .with_workaround(
                                    "wrap the binding in a `let` block for local AoT codegen, or wait for lazy global initialization support",
                                ),
                            ));
                        }
                        let init = self.convert_expr(value)?;
                        let global = AotGlobal::with_init(var.clone(), ty.clone(), init);
                        aot_program.add_global(global);
                        self.declared_locals.insert(var.clone());
                        // Register in type environment for later lookups (e.g., tuple indexing)
                        self.engine.env.insert(var.clone(), ty);
                    } else {
                        // It's a reassignment in main
                        let expanded = self.convert_stmt_expanded(stmt)?;
                        aot_program.main.extend(expanded);
                    }
                }
                _ => {
                    let expanded = self.convert_stmt_expanded(stmt)?;
                    aot_program.main.extend(expanded);
                }
            }
        }

        Ok(aot_program)
    }

    fn function_redefinition_key(func: &Function) -> String {
        let positional = func
            .params
            .iter()
            .map(|param| format!("{}:{}", param.name, param.effective_type()));
        let keyword = func
            .kwparams
            .iter()
            .filter(|param| !param.is_varargs)
            .map(|param| format!("{}:{}", param.name, param.effective_type()));
        format!(
            "{}({})",
            func.name,
            positional.chain(keyword).collect::<Vec<_>>().join(",")
        )
    }

    /// Convert an enum definition to AoT enum
    ///
    /// Julia enums are integer-backed symbolic types created with `@enum`.
    /// Each member has a unique Int32 value.
    fn convert_enum(enum_def: &EnumDef) -> AotEnum {
        let mut aot_enum = AotEnum::new(enum_def.name.clone());
        for member in &enum_def.members {
            aot_enum.add_member(member.name.clone(), member.value as i32);
        }
        aot_enum
    }

    /// Convert a struct definition
    fn convert_struct(&self, struct_def: &StructDef) -> AotResult<AotStruct> {
        let mut aot_struct = AotStruct::new(struct_def.name.clone(), struct_def.is_mutable)
            .with_type_params(
                struct_def
                    .type_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect(),
            );

        for field in &struct_def.fields {
            let ty = if struct_def.name == "Complex" {
                // Complex{T<:Real} has type-variable fields (re::T, im::T).
                // as_julia_type() returns None for type variables, falling back to Any/Value.
                // AoT codegen hardcodes Complex Add/Mul operators that require f64 fields,
                // so force F64 here to match (Issue #3407).
                StaticType::F64
            } else if let Some(type_expr) = &field.type_expr {
                match type_expr {
                    TypeExpr::Concrete(jt) => self.julia_type_to_static(jt),
                    TypeExpr::TypeVar(name)
                        if struct_def
                            .type_params
                            .iter()
                            .any(|param| param.name == *name) =>
                    {
                        StaticType::Struct {
                            type_id: 0,
                            name: name.clone(),
                        }
                    }
                    _ => StaticType::from_type_expr_lossy(type_expr),
                }
            } else {
                StaticType::Any
            };
            aot_struct.add_field(field.name.clone(), ty);
        }

        Ok(aot_struct)
    }

    fn collect_used_parametric_structs(program: &Program) -> HashSet<String> {
        let parametric_names: HashSet<_> = program
            .structs
            .iter()
            .filter(|struct_def| struct_def.is_parametric())
            .map(|struct_def| struct_def.name.clone())
            .collect();
        let mut used = HashSet::new();
        Self::collect_used_parametric_structs_in_block(&program.main, &parametric_names, &mut used);
        for func in &program.functions {
            Self::collect_used_parametric_structs_in_block(
                &func.body,
                &parametric_names,
                &mut used,
            );
        }
        used
    }

    fn collect_used_parametric_structs_in_block(
        block: &Block,
        parametric_names: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { value, .. } | Stmt::Expr { expr: value, .. } => {
                    Self::collect_used_parametric_structs_in_expr(value, parametric_names, used);
                }
                Stmt::Return {
                    value: Some(value), ..
                } => Self::collect_used_parametric_structs_in_expr(value, parametric_names, used),
                Stmt::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    Self::collect_used_parametric_structs_in_expr(
                        condition,
                        parametric_names,
                        used,
                    );
                    Self::collect_used_parametric_structs_in_block(
                        then_branch,
                        parametric_names,
                        used,
                    );
                    if let Some(else_branch) = else_branch {
                        Self::collect_used_parametric_structs_in_block(
                            else_branch,
                            parametric_names,
                            used,
                        );
                    }
                }
                Stmt::While {
                    condition, body, ..
                } => {
                    Self::collect_used_parametric_structs_in_expr(
                        condition,
                        parametric_names,
                        used,
                    );
                    Self::collect_used_parametric_structs_in_block(body, parametric_names, used);
                }
                Stmt::For {
                    start,
                    end,
                    step,
                    body,
                    ..
                } => {
                    Self::collect_used_parametric_structs_in_expr(start, parametric_names, used);
                    Self::collect_used_parametric_structs_in_expr(end, parametric_names, used);
                    if let Some(step) = step {
                        Self::collect_used_parametric_structs_in_expr(step, parametric_names, used);
                    }
                    Self::collect_used_parametric_structs_in_block(body, parametric_names, used);
                }
                Stmt::ForEach { iterable, body, .. } => {
                    Self::collect_used_parametric_structs_in_expr(iterable, parametric_names, used);
                    Self::collect_used_parametric_structs_in_block(body, parametric_names, used);
                }
                Stmt::Block(inner) => {
                    Self::collect_used_parametric_structs_in_block(inner, parametric_names, used);
                }
                _ => {}
            }
        }
    }

    fn collect_used_parametric_structs_in_expr(
        expr: &Expr,
        parametric_names: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        match expr {
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                let base = StaticType::parametric_type_parts(function)
                    .map(|(base, _)| base)
                    .unwrap_or(function.as_str());
                if parametric_names.contains(base) {
                    used.insert(base.to_string());
                }
                for arg in args {
                    Self::collect_used_parametric_structs_in_expr(arg, parametric_names, used);
                }
                for (_, arg) in kwargs {
                    Self::collect_used_parametric_structs_in_expr(arg, parametric_names, used);
                }
            }
            Expr::AssignExpr { value, .. }
            | Expr::UnaryOp { operand: value, .. }
            | Expr::FieldAccess { object: value, .. } => {
                Self::collect_used_parametric_structs_in_expr(value, parametric_names, used);
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_used_parametric_structs_in_expr(left, parametric_names, used);
                Self::collect_used_parametric_structs_in_expr(right, parametric_names, used);
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::collect_used_parametric_structs_in_expr(condition, parametric_names, used);
                Self::collect_used_parametric_structs_in_expr(then_expr, parametric_names, used);
                Self::collect_used_parametric_structs_in_expr(else_expr, parametric_names, used);
            }
            Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
                for elem in elements {
                    Self::collect_used_parametric_structs_in_expr(elem, parametric_names, used);
                }
            }
            Expr::NamedTupleLiteral { fields, .. } => {
                for (_, field) in fields {
                    Self::collect_used_parametric_structs_in_expr(field, parametric_names, used);
                }
            }
            Expr::Index { array, indices, .. } => {
                Self::collect_used_parametric_structs_in_expr(array, parametric_names, used);
                for index in indices {
                    Self::collect_used_parametric_structs_in_expr(index, parametric_names, used);
                }
            }
            Expr::Range {
                start, stop, step, ..
            } => {
                Self::collect_used_parametric_structs_in_expr(start, parametric_names, used);
                Self::collect_used_parametric_structs_in_expr(stop, parametric_names, used);
                if let Some(step) = step {
                    Self::collect_used_parametric_structs_in_expr(step, parametric_names, used);
                }
            }
            Expr::LetBlock { bindings, body, .. } => {
                for (_, value) in bindings {
                    Self::collect_used_parametric_structs_in_expr(value, parametric_names, used);
                }
                Self::collect_used_parametric_structs_in_block(body, parametric_names, used);
            }
            Expr::Comprehension {
                body, iter, filter, ..
            }
            | Expr::Generator {
                body, iter, filter, ..
            } => {
                Self::collect_used_parametric_structs_in_expr(body, parametric_names, used);
                Self::collect_used_parametric_structs_in_expr(iter, parametric_names, used);
                if let Some(filter) = filter {
                    Self::collect_used_parametric_structs_in_expr(filter, parametric_names, used);
                }
            }
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                Self::collect_used_parametric_structs_in_expr(body, parametric_names, used);
                for (_, iter) in iterations {
                    Self::collect_used_parametric_structs_in_expr(iter, parametric_names, used);
                }
                if let Some(filter) = filter {
                    Self::collect_used_parametric_structs_in_expr(filter, parametric_names, used);
                }
            }
            Expr::Builtin { args, .. } => {
                for arg in args {
                    Self::collect_used_parametric_structs_in_expr(arg, parametric_names, used);
                }
            }
            _ => {}
        }
    }

    /// Convert a function definition
    pub(crate) fn convert_function(&mut self, func: &Function) -> AotResult<AotFunction> {
        // Get type information from inference
        let typed_func = self.next_typed_function_for(func);
        // The typed signature already carries keyword parameters as trailing
        // positional parameters (Issue #7042), so `params` includes them and
        // the call site fills them in declaration order.
        let (params, return_type, is_generic, local_types) = if let Some(typed_func) = typed_func {
            let params: Vec<_> = typed_func
                .signature
                .param_names
                .iter()
                .zip(typed_func.signature.param_types.iter())
                .map(|(n, t)| (n.clone(), t.clone()))
                .collect();
            (
                params,
                typed_func.signature.return_type.clone(),
                typed_func.signature.inference_level > 2,
                typed_func.locals.clone(),
            )
        } else {
            self.infer_function_types(func)
        };

        let mut aot_func = AotFunction::new(func.name.clone(), params.clone(), return_type.clone());
        aot_func.is_generic = is_generic;
        aot_func.inline_policy = Self::inline_policy_from_meta(&func.body.stmts);

        // Set up local variable scope
        self.declared_locals.clear();
        for (name, ty) in &params {
            self.declared_locals.insert(name.clone());
            self.engine.env.insert(name.clone(), ty.clone());
        }
        for (name, ty) in &local_types {
            self.engine.env.insert(name.clone(), ty.clone());
        }

        // Track the function's return type for type coercion in return statements
        self.current_return_type = Some(return_type);

        // Convert function body
        for stmt in &func.body.stmts {
            let expanded = self.convert_stmt_expanded(stmt)?;
            aot_func.body.extend(expanded);
        }

        // Clear return type after function conversion
        self.current_return_type = None;

        Ok(aot_func)
    }

    fn next_typed_function_for(&mut self, func: &Function) -> Option<&'a TypedFunction> {
        let declared_param_types = self.declared_static_param_types(func);
        let funcs = self.typed.get_functions(&func.name)?;
        if !self.generic_any_function_names.contains(&func.name) {
            // Match the typed signature by arity, not just the first one. A
            // default-argument stub (`f(x) = f(x, 10)`) and its full method
            // (`f(x, y)`) share a name; without arity matching the stub would
            // take the full method's 2-param signature, collapse onto it during
            // method-table dedup, and the `f(x)` call site would emit a 1-arg
            // call to a 2-param function (Issue #7044). Keyword params count
            // toward arity since they are modeled as trailing positionals
            // (Issue #7042). For same-arity typed overloads, prefer the typed
            // function whose inferred signature matches explicit source
            // annotations so `f(::Float64)` does not reuse the first `f(::Int64)`
            // signature (Issue #7387).
            if let Some(declared_param_types) = declared_param_types.as_ref() {
                if let Some(typed_func) = funcs
                    .iter()
                    .find(|tf| tf.signature.param_types == *declared_param_types)
                {
                    return Some(typed_func);
                }
            }
            let arity = func.params.len() + func.kwparams.iter().filter(|k| !k.is_varargs).count();
            return funcs
                .iter()
                .find(|tf| tf.signature.param_names.len() == arity)
                .or_else(|| funcs.first());
        }

        let occurrence = self
            .function_occurrences
            .entry(func.name.clone())
            .or_insert(0);
        let typed_func = funcs.get(*occurrence)?;
        *occurrence += 1;
        Some(typed_func)
    }

    fn declared_static_param_types(&self, func: &Function) -> Option<Vec<StaticType>> {
        let mut has_explicit_annotation = false;
        let mut param_types = Vec::new();
        for param in &func.params {
            has_explicit_annotation |= param.type_annotation.is_some();
            param_types.push(self.julia_type_to_static(&param.effective_type()));
        }
        for param in func.kwparams.iter().filter(|param| !param.is_varargs) {
            has_explicit_annotation |= param.type_annotation.is_some();
            param_types.push(self.julia_type_to_static(&param.effective_type()));
        }
        has_explicit_annotation.then_some(param_types)
    }

    fn inline_policy_from_meta(stmts: &[Stmt]) -> AotInlinePolicy {
        let mut policy = AotInlinePolicy::Auto;
        for stmt in stmts {
            let Stmt::Meta { annotation, .. } = stmt else {
                continue;
            };
            match annotation.name.as_str() {
                "noinline" => policy = AotInlinePolicy::Never,
                "inline" if policy != AotInlinePolicy::Never => policy = AotInlinePolicy::Always,
                _ => {}
            }
        }
        policy
    }

    /// Infer function types when not available from TypedProgram
    fn infer_function_types(
        &self,
        func: &Function,
    ) -> (
        Vec<(String, StaticType)>,
        StaticType,
        bool,
        HashMap<String, StaticType>,
    ) {
        let params: Vec<_> = func
            .params
            .iter()
            .map(|p| {
                let ty = self.julia_type_to_static(&p.effective_type());
                (p.name.clone(), ty)
            })
            .collect();

        let return_type = func
            .return_type
            .as_ref()
            .map(|jt| self.julia_type_to_static(jt))
            .unwrap_or(StaticType::Any);

        let is_generic = params.iter().any(|(_, t)| matches!(t, StaticType::Any));

        (params, return_type, is_generic, HashMap::new())
    }
}
