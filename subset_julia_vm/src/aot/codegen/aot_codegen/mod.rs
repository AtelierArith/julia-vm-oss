//! High-level AoT IR to Rust code generator.
//!
//! This module implements `AotCodeGenerator` which generates Rust code
//! from the high-level AoT IR (`AotProgram`, `AotFunction`).
#![allow(clippy::cast_sign_loss)] // known-safe index/counter casts (i64/i32->usize)

mod control_flow;
mod expressions;
mod operations;
mod program;
mod statements;
#[cfg(test)]
mod tests;

use super::CodegenConfig;
use crate::aot::abi::AotAbiValue;
use crate::aot::ir::{AotExpr, AotFunction, AotProgram, AotStmt};
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use std::collections::{HashMap, HashSet};

/// Escape identifiers that are Rust reserved keywords by prefixing with `r#`.
pub(crate) fn escape_rust_ident(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let sanitized = if sanitized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        format!("_{}", sanitized)
    } else {
        sanitized
    };

    // `self`/`super`/`crate`/`Self` cannot be used even as raw identifiers, so
    // rename them instead of emitting invalid `r#self` style bindings.
    const NON_RAW_IDENT_KEYWORDS: &[&str] = &["self", "super", "crate", "Self"];
    if NON_RAW_IDENT_KEYWORDS.contains(&sanitized.as_str()) {
        return format!("_{}", sanitized);
    }

    // Rust strict, reserved, and weak/future keywords that are legal raw identifiers.
    const RAW_IDENT_KEYWORDS: &[&str] = &[
        "as",
        "break",
        "const",
        "continue",
        "else",
        "enum",
        "extern",
        "false",
        "fn",
        "for",
        "if",
        "impl",
        "in",
        "let",
        "loop",
        "match",
        "mod",
        "move",
        "mut",
        "pub",
        "ref",
        "return",
        "static",
        "struct",
        "trait",
        "true",
        "type",
        "unsafe",
        "use",
        "where",
        "while",
        // Reserved for future use
        "abstract",
        "async",
        "await",
        "become",
        "box",
        "do",
        "dyn",
        "final",
        "gen",
        "macro",
        "macro_rules",
        "override",
        "priv",
        "try",
        "typeof",
        "union",
        "unsized",
        "virtual",
        "yield",
    ];
    if RAW_IDENT_KEYWORDS.contains(&sanitized.as_str()) {
        format!("r#{}", sanitized)
    } else {
        sanitized
    }
}

/// Prefix applied to top-level globals emitted as Rust `static`s so they cannot
/// be shadowed by a function parameter of the same name (E0530; Issue #7242).
/// A short, common Julia name (`x`/`a`/`b`) would otherwise collide with prelude
/// helpers such as `op_add(a: f64, b: f64)`.
pub(super) const GLOBAL_STATIC_PREFIX: &str = "__sjulia_global_";

/// The Rust identifier used for a top-level global `static`. The name is first
/// run through `escape_rust_ident` (so keyword/punctuation handling stays
/// consistent), then given the collision-free `__sjulia_global_` prefix
/// (Issue #7242).
pub(super) fn global_static_ident(name: &str) -> String {
    format!("{}{}", GLOBAL_STATIC_PREFIX, escape_rust_ident(name))
}

/// AoT Code Generator for high-level IR
///
/// Generates Rust code from AotProgram, AotFunction, AotStmt, and AotExpr.
#[derive(Debug)]
pub struct AotCodeGenerator {
    /// Configuration
    pub(super) config: CodegenConfig,
    /// Output buffer
    pub(super) output: String,
    /// Current indentation level
    pub(super) indent_level: usize,
    /// Functions that have multiple methods (require mangled names)
    pub(super) multidispatch_funcs: HashSet<String>,
    /// Method table: function name -> list of (mangled_name, param_types, return_type)
    pub(super) method_table: HashMap<String, Vec<(String, Vec<StaticType>, StaticType)>>,
    /// Original source method count by function name before signature deduplication.
    pub(super) function_method_counts: HashMap<String, usize>,
    /// Return type for the function currently being emitted, used for explicit returns.
    pub(super) current_function_return_type: Option<StaticType>,
    /// Names of top-level globals emitted as Rust `static`s. References to these
    /// are rewritten to a collision-free `__sjulia_global_<name>` so a function
    /// parameter of the same name cannot shadow the static (E0530; Issue #7242).
    pub(super) global_names: HashSet<String>,
    /// Parameter names of the function currently being emitted. A parameter
    /// shadows any same-named global within its body, so such references must
    /// NOT be rewritten to the global static (Issue #7242).
    pub(super) current_function_param_names: HashSet<String>,
    /// Locals whose declaration was hoisted to a deferred `let mut x: T;` at the
    /// top of the current function because their first assignment is inside a
    /// nested control-flow block yet they are referenced from another scope.
    /// Their in-block `Let`s are emitted as plain assignments (Issue #8181).
    pub(super) current_function_hoisted_locals: HashSet<String>,
}

impl AotCodeGenerator {
    /// Create a new AoT code generator
    pub fn new(config: CodegenConfig) -> Self {
        Self {
            config,
            output: String::new(),
            indent_level: 0,
            multidispatch_funcs: HashSet::new(),
            method_table: HashMap::new(),
            function_method_counts: HashMap::new(),
            current_function_return_type: None,
            global_names: HashSet::new(),
            current_function_param_names: HashSet::new(),
            current_function_hoisted_locals: HashSet::new(),
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(CodegenConfig::default())
    }

    /// Write a line with current indentation
    pub(super) fn write_line(&mut self, line: &str) {
        for _ in 0..self.indent_level {
            self.output.push_str(&self.config.indent);
        }
        self.output.push_str(line);
        self.output.push('\n');
    }

    /// Write without newline
    #[allow(dead_code)] // retained codegen helper API
    pub(super) fn write(&mut self, text: &str) {
        self.output.push_str(text);
    }

    /// Write a blank line
    pub(super) fn blank_line(&mut self) {
        self.output.push('\n');
    }

    /// Increase indentation
    pub(super) fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease indentation
    pub(super) fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Get current indentation string
    #[allow(dead_code)] // retained codegen helper API
    pub(super) fn current_indent(&self) -> String {
        self.config.indent.repeat(self.indent_level)
    }

    /// Get constant index value from an expression (for tuple indexing)
    ///
    /// Returns Some(index) if the expression is a constant integer literal,
    /// None otherwise. Used for generating Rust tuple field access syntax.
    #[allow(dead_code)] // retained codegen helper API
    pub(super) fn get_const_index(expr: &AotExpr) -> Option<usize> {
        match expr {
            AotExpr::LitI64(v) => Some(*v as usize),
            AotExpr::LitI32(v) => Some(*v as usize),
            _ => None,
        }
    }

    // ========== Type Generation ==========

    /// Generate type annotation
    pub(super) fn type_to_rust(&self, ty: &StaticType) -> String {
        AotAbiValue::from_static_type(ty).rust_type().to_string()
    }

    // ========== Program Generation ==========

    /// Generate a complete AoT program
    pub fn generate_program(&mut self, program: &AotProgram) -> AotResult<String> {
        self.output.clear();
        self.indent_level = 0;

        // Build method table for multiple dispatch
        self.build_method_table(program);

        // Record top-level global names so references to them are rewritten to
        // the collision-free `__sjulia_global_<name>` static (Issue #7242).
        self.global_names = program.globals.iter().map(|g| g.name.clone()).collect();

        let c_abi_exports = self.resolve_c_abi_exports(program)?;
        let direct_c_abi_exports: HashSet<_> = c_abi_exports
            .iter()
            .filter(|export| export.export_name == export.rust_func_name)
            .map(|export| export.rust_func_name.clone())
            .collect();

        // Emit prelude
        self.emit_prelude();

        // Emit struct definitions
        let has_complex = Self::program_uses_complex(program);
        // Emit struct definitions in dependency order so field types are declared
        // before structs that refer to them (Issue #6974).
        let ordered_structs = Self::ordered_structs_by_dependency(&program.structs)?;
        for s in ordered_structs {
            self.emit_struct(s)?;
            self.blank_line();
        }
        if has_complex && !program.structs.iter().any(|s| s.name == "Complex") {
            self.emit_complex_struct();
            self.blank_line();
        }

        // Emit struct-dependent prelude (Complex operators, im constant, etc.)
        // Must come after struct definitions so types are available (Issue #3410).
        self.emit_struct_dependent_prelude(has_complex);

        // Emit enum definitions (as i32 constants)
        for e in &program.enums {
            self.emit_enum(e)?;
            self.blank_line();
        }

        // Emit global variables
        for global in &program.globals {
            self.emit_global(global)?;
        }
        if !program.globals.is_empty() {
            self.blank_line();
        }

        // Check if user defined a main function
        let has_user_main = program.functions.iter().any(|f| f.name == "main");

        // Emit function definitions (with mangled names for multidispatch).
        // Deduplicate: multiple Julia methods may resolve to the same mangled
        // Rust name when their concrete type signatures are identical.  Emitting
        // the same function twice causes a Rust compile error, so we keep only
        // the first occurrence.
        let mut emitted_func_names: HashSet<String> = HashSet::new();
        for func in &program.functions {
            let func_name = self.emitted_function_name(func);
            let direct_c_abi_export = direct_c_abi_exports.contains(&func_name);
            if !emitted_func_names.insert(func_name.clone()) {
                continue;
            }
            self.emit_function(func, direct_c_abi_export)?;
            self.blank_line();
        }

        // Emit dispatcher functions for multiple dispatch
        self.emit_dispatchers()?;

        // Emit alias C ABI wrappers for explicitly exported functions.
        self.emit_c_abi_wrappers(&c_abi_exports)?;

        // Emit main function only if user didn't define one
        // If user defined main(), it becomes the entry point and we skip emit_main
        // to avoid duplicate main function definitions
        if !has_user_main {
            self.emit_main(&program.main)?;
        }

        Ok(std::mem::take(&mut self.output))
    }

    fn program_uses_complex(program: &AotProgram) -> bool {
        program.structs.iter().any(|s| s.name == "Complex")
            || program
                .globals
                .iter()
                .any(|global| Self::type_uses_complex(&global.ty))
            || program.functions.iter().any(Self::function_uses_complex)
            || Self::stmts_use_complex(&program.main)
    }

    fn function_uses_complex(func: &AotFunction) -> bool {
        Self::type_uses_complex(&func.return_type)
            || func
                .params
                .iter()
                .any(|(_, ty)| Self::type_uses_complex(ty))
            || Self::stmts_use_complex(&func.body)
    }

    fn stmts_use_complex(stmts: &[AotStmt]) -> bool {
        stmts.iter().any(Self::stmt_uses_complex)
    }

    fn stmt_uses_complex(stmt: &AotStmt) -> bool {
        match stmt {
            AotStmt::Let { ty, value, .. } => {
                Self::type_uses_complex(ty) || Self::expr_uses_complex(value)
            }
            AotStmt::Assign { target, value } => {
                Self::expr_uses_complex(target) || Self::expr_uses_complex(value)
            }
            AotStmt::CompoundAssign { target, value, .. } => {
                Self::expr_uses_complex(target) || Self::expr_uses_complex(value)
            }
            AotStmt::Expr(expr) | AotStmt::ValueCarrier(expr) => Self::expr_uses_complex(expr),
            AotStmt::Return(expr) => expr.as_ref().is_some_and(Self::expr_uses_complex),
            AotStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::expr_uses_complex(condition)
                    || Self::stmts_use_complex(then_branch)
                    || else_branch.as_deref().is_some_and(Self::stmts_use_complex)
            }
            AotStmt::While { condition, body } => {
                Self::expr_uses_complex(condition) || Self::stmts_use_complex(body)
            }
            AotStmt::ForRange {
                start,
                stop,
                step,
                body,
                ..
            } => {
                Self::expr_uses_complex(start)
                    || Self::expr_uses_complex(stop)
                    || step.as_ref().is_some_and(Self::expr_uses_complex)
                    || Self::stmts_use_complex(body)
            }
            AotStmt::ForEach { iter, body, .. } => {
                Self::expr_uses_complex(iter) || Self::stmts_use_complex(body)
            }
            AotStmt::Break | AotStmt::Continue => false,
        }
    }

    fn expr_uses_complex(expr: &AotExpr) -> bool {
        if Self::type_uses_complex(&expr.get_type()) {
            return true;
        }

        match expr {
            AotExpr::Var { ty, .. } => Self::type_uses_complex(ty),
            AotExpr::BinOpStatic { left, right, .. }
            | AotExpr::BinOpDynamic { left, right, .. } => {
                Self::expr_uses_complex(left) || Self::expr_uses_complex(right)
            }
            AotExpr::UnaryOp { operand, .. } => Self::expr_uses_complex(operand),
            AotExpr::CallStatic {
                args, return_ty, ..
            } => Self::type_uses_complex(return_ty) || args.iter().any(Self::expr_uses_complex),
            AotExpr::CallDynamic { args, .. } | AotExpr::CallBuiltin { args, .. } => {
                args.iter().any(Self::expr_uses_complex)
            }
            AotExpr::ArrayLit {
                elements, elem_ty, ..
            } => Self::type_uses_complex(elem_ty) || elements.iter().any(Self::expr_uses_complex),
            AotExpr::SetFromIter { iter, elem_ty } => {
                Self::type_uses_complex(elem_ty) || Self::expr_uses_complex(iter)
            }
            AotExpr::Comprehension {
                iter,
                body,
                filter,
                elem_ty,
                ..
            }
            | AotExpr::Generator {
                iter,
                body,
                filter,
                elem_ty,
                ..
            } => {
                Self::type_uses_complex(elem_ty)
                    || Self::expr_uses_complex(iter)
                    || Self::expr_uses_complex(body)
                    || filter
                        .as_ref()
                        .is_some_and(|expr| Self::expr_uses_complex(expr))
            }
            AotExpr::MultiComprehension {
                iterations,
                body,
                filter,
                elem_ty,
            } => {
                Self::type_uses_complex(elem_ty)
                    || iterations
                        .iter()
                        .any(|(_, iter)| Self::expr_uses_complex(iter))
                    || Self::expr_uses_complex(body)
                    || filter
                        .as_ref()
                        .is_some_and(|expr| Self::expr_uses_complex(expr))
            }
            AotExpr::TupleLit { elements } => elements.iter().any(Self::expr_uses_complex),
            AotExpr::NamedTupleLit { fields } => {
                fields.iter().any(|(_, expr)| Self::expr_uses_complex(expr))
            }
            AotExpr::Index {
                array,
                indices,
                elem_ty,
                ..
            } => {
                Self::type_uses_complex(elem_ty)
                    || Self::expr_uses_complex(array)
                    || indices.iter().any(Self::expr_uses_complex)
            }
            AotExpr::Range {
                start,
                stop,
                step,
                elem_ty,
            } => {
                Self::type_uses_complex(elem_ty)
                    || Self::expr_uses_complex(start)
                    || Self::expr_uses_complex(stop)
                    || step
                        .as_ref()
                        .is_some_and(|expr| Self::expr_uses_complex(expr))
            }
            AotExpr::StructNew { fields, .. } => fields.iter().any(Self::expr_uses_complex),
            AotExpr::FieldAccess {
                object, field_ty, ..
            } => Self::type_uses_complex(field_ty) || Self::expr_uses_complex(object),
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                result_ty,
            } => {
                Self::type_uses_complex(result_ty)
                    || Self::expr_uses_complex(condition)
                    || Self::expr_uses_complex(then_expr)
                    || Self::expr_uses_complex(else_expr)
            }
            AotExpr::Box(inner) => Self::expr_uses_complex(inner),
            AotExpr::Unbox { value, target_ty } | AotExpr::Convert { value, target_ty } => {
                Self::type_uses_complex(target_ty) || Self::expr_uses_complex(value)
            }
            AotExpr::Lambda {
                params,
                body,
                captures,
                return_ty,
            } => {
                Self::type_uses_complex(return_ty)
                    || params.iter().any(|(_, ty)| Self::type_uses_complex(ty))
                    || captures.iter().any(|(_, ty)| Self::type_uses_complex(ty))
                    || Self::stmts_use_complex(body)
            }
            AotExpr::LitI64(_)
            | AotExpr::LitI32(_)
            | AotExpr::LitF64(_)
            | AotExpr::LitF32(_)
            | AotExpr::LitBool(_)
            | AotExpr::LitStr(_)
            | AotExpr::LitChar(_)
            | AotExpr::LitNothing
            | AotExpr::LitMissing => false,
        }
    }

    fn type_uses_complex(ty: &StaticType) -> bool {
        matches!(
            ty,
            StaticType::Struct { name, .. }
                if name == "Complex"
                    || StaticType::complex_param_type_from_name(name).is_some()
        )
    }

    /// Generate a single function (convenience method)
    pub fn generate_function(&mut self, func: &AotFunction) -> AotResult<String> {
        self.output.clear();
        self.indent_level = 0;
        self.emit_function(func, false)?;
        Ok(std::mem::take(&mut self.output))
    }
}
