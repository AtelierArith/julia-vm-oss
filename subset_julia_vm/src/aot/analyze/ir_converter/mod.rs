//! IR conversion from Core IR to AoT IR.
//!
//! The `IrConverter` translates the Core IR representation into AoT IR,
//! performing type annotation and specialization based on inference results.

use super::super::inference::{TypeInferenceEngine, TypedProgram};
use super::super::ir::{
    AotBinOp, AotBuiltinOp, AotEnum, AotExpr, AotFunction, AotGlobal, AotProgram, AotStmt,
    AotStruct, AotUnaryOp,
};
use super::super::types::StaticType;
use super::super::AotResult;
use crate::ir::core::{Block, EnumDef, Expr, Function, Literal, Program, Stmt, StructDef};
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
    /// Current function's return type (for type coercion in return statements)
    current_return_type: Option<StaticType>,
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
        // Copy globals from typed program for tuple indexing
        engine.env = typed.globals.clone();

        // Build function lookup map for lambda conversion
        let functions: HashMap<String, &'a Function> = program
            .functions
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        Self {
            typed,
            engine,
            declared_locals: HashSet::new(),
            functions,
            current_return_type: None,
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
            let aot_struct = self.convert_struct(struct_def)?;
            aot_program.add_struct(aot_struct);
        }

        // Convert enum definitions
        for enum_def in &program.enums {
            let aot_enum = Self::convert_enum(enum_def);
            aot_program.add_enum(aot_enum);
        }

        // Convert functions, excluding base library functions that are handled by AoT builtins
        for func in &program.functions {
            // Skip base library operator/convert functions - these are handled as AoT builtins
            if Self::is_aot_builtin_function(&func.name) && func.is_base_extension {
                continue;
            }
            let aot_func = self.convert_function(func)?;
            aot_program.add_function(aot_func);
        }

        // Convert main block statements to globals and main execution
        self.declared_locals.clear();
        for stmt in &program.main.stmts {
            match stmt {
                Stmt::Assign { var, value, .. } => {
                    // Check if this is a global variable declaration
                    if !self.declared_locals.contains(var) {
                        let ty = self.engine.infer_expr_type(value);
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
        let mut aot_struct = AotStruct::new(struct_def.name.clone(), struct_def.is_mutable);

        for field in &struct_def.fields {
            let ty = if struct_def.name == "Complex" {
                // Complex{T<:Real} has type-variable fields (re::T, im::T).
                // as_julia_type() returns None for type variables, falling back to Any/Value.
                // AoT codegen hardcodes Complex Add/Mul operators that require f64 fields,
                // so force F64 here to match (Issue #3407).
                StaticType::F64
            } else if let Some(jt) = field.as_julia_type() {
                self.julia_type_to_static(&jt)
            } else {
                StaticType::Any
            };
            aot_struct.add_field(field.name.clone(), ty);
        }

        Ok(aot_struct)
    }

    /// Convert a function definition
    pub(crate) fn convert_function(&mut self, func: &Function) -> AotResult<AotFunction> {
        // Get type information from inference
        let typed_funcs = self.typed.get_functions(&func.name);
        let (params, return_type, is_generic) = if let Some(funcs) = typed_funcs {
            if let Some(typed_func) = funcs.first() {
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
                )
            } else {
                self.infer_function_types(func)
            }
        } else {
            self.infer_function_types(func)
        };

        let mut aot_func = AotFunction::new(func.name.clone(), params.clone(), return_type.clone());
        aot_func.is_generic = is_generic;

        // Set up local variable scope
        self.declared_locals.clear();
        for (name, _) in &params {
            self.declared_locals.insert(name.clone());
            self.engine.env.insert(
                name.clone(),
                params.iter().find(|(n, _)| n == name).unwrap().1.clone(),
            );
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

    /// Infer function types when not available from TypedProgram
    fn infer_function_types(
        &self,
        func: &Function,
    ) -> (Vec<(String, StaticType)>, StaticType, bool) {
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

        (params, return_type, is_generic)
    }
}
