use super::AotCodeGenerator;
use crate::aot::ir::{
    AotEnum, AotExpr, AotFunction, AotGlobal, AotProgram, AotStmt, AotStruct,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use std::collections::{HashMap, HashSet};

impl AotCodeGenerator {
    /// Build the method table for multiple dispatch
    pub(super) fn build_method_table(&mut self, program: &AotProgram) {
        self.multidispatch_funcs.clear();
        self.method_table.clear();

        // Group functions by name
        let mut func_groups: HashMap<String, Vec<&AotFunction>> = HashMap::new();
        for func in &program.functions {
            func_groups.entry(func.name.clone()).or_default().push(func);
        }

        // Identify functions with multiple methods
        for (name, methods) in func_groups {
            if methods.len() > 1 {
                self.multidispatch_funcs.insert(name.clone());
            }

            // Build method table entry
            let entries: Vec<_> = methods
                .iter()
                .map(|f| {
                    let param_types: Vec<_> = f.params.iter().map(|(_, ty)| ty.clone()).collect();
                    (f.mangled_name(), param_types)
                })
                .collect();
            self.method_table.insert(name, entries);
        }
    }

    /// Check if a function requires multiple dispatch
    pub(super) fn needs_dispatch(&self, func_name: &str) -> bool {
        self.multidispatch_funcs.contains(func_name)
    }

    /// Emit dispatcher functions for all multidispatch functions
    pub(super) fn emit_dispatchers(&mut self) -> AotResult<()> {
        // Clone to avoid borrow issues
        let multidispatch: Vec<_> = self.multidispatch_funcs.iter().cloned().collect();

        for func_name in multidispatch {
            if let Some(methods) = self.method_table.get(&func_name).cloned() {
                self.emit_dispatcher(&func_name, &methods)?;
                self.blank_line();
            }
        }
        Ok(())
    }

    /// Emit a dispatcher function for a multidispatch function
    fn emit_dispatcher(
        &mut self,
        func_name: &str,
        methods: &[(String, Vec<StaticType>)],
    ) -> AotResult<()> {
        if methods.is_empty() {
            return Ok(());
        }

        // Use the first method to determine parameter count
        let _param_count = methods[0].1.len();

        if self.config.emit_comments {
            self.write_line(&format!(
                "// Dispatcher for {} with {} methods",
                func_name,
                methods.len()
            ));
        }

        // For now, generate a comment-only dispatcher since full dynamic dispatch
        // requires runtime value types. The actual dispatch happens at call sites
        // when types are statically known.
        self.write_line(&format!(
            "// Multiple dispatch: {} has {} method(s):",
            func_name,
            methods.len()
        ));
        for (mangled_name, param_types) in methods {
            let type_sig: Vec<_> = param_types.iter().map(|t| t.julia_type_name()).collect();
            self.write_line(&format!("//   - {}({})", mangled_name, type_sig.join(", ")));
        }

        // If we want to generate a dynamic dispatcher (for cases where types aren't known),
        // we would need a Value enum. For now, we rely on static dispatch resolution.
        // This keeps the generated code simpler and more efficient.

        Ok(())
    }

    /// Resolve static dispatch for a function call
    ///
    /// Given a function name and argument types, returns the mangled name of the
    /// method that matches those argument types. If no exact match is found,
    /// returns the original function name (which may fail at compile time).
    pub(super) fn resolve_dispatch(&self, func_name: &str, arg_types: &[StaticType]) -> String {
        if let Some(methods) = self.method_table.get(func_name) {
            // Try to find an exact match
            for (mangled_name, param_types) in methods {
                if param_types.len() == arg_types.len() {
                    let matches = param_types
                        .iter()
                        .zip(arg_types.iter())
                        .all(|(param, arg)| self.types_match(param, arg));
                    if matches {
                        return mangled_name.clone();
                    }
                }
            }

            // No exact match found, generate mangled name from arg types
            // This may fail at link time if no such method exists
            if !arg_types.is_empty() {
                let type_suffix: Vec<_> = arg_types.iter().map(|t| t.mangle_suffix()).collect();
                return format!("{}_{}", func_name, type_suffix.join("_"));
            }
        }

        // No dispatch needed or no methods found
        AotFunction::sanitize_function_name(func_name)
    }

    /// Check if two types match for dispatch resolution
    fn types_match(&self, expected: &StaticType, actual: &StaticType) -> bool {
        // Exact match
        if expected == actual {
            return true;
        }

        // Any type matches anything
        if matches!(expected, StaticType::Any) || matches!(actual, StaticType::Any) {
            return true;
        }

        // For now, require exact type match for dispatch
        // In the future, we could add subtyping rules here
        false
    }

    /// Check if a string represents a closure literal
    ///
    /// Closures start with `|` or `move |` in Rust syntax.
    pub(super) fn is_closure_literal(s: &str) -> bool {
        let trimmed = s.trim();
        trimmed.starts_with('|') || trimmed.starts_with("move |")
    }

    /// Emit prelude (imports and setup)
    pub(super) fn emit_prelude(&mut self) {
        self.write_line("//! Auto-generated by SubsetJuliaVM AoT compiler");
        self.write_line("//! Do not edit manually");
        self.blank_line();
        self.write_line("#![allow(unused_variables)]");
        self.write_line("#![allow(unused_mut)]");
        self.write_line("#![allow(dead_code)]");
        self.blank_line();

        // AoT broadcast helpers used by ir_converter broadcast lowering.
        self.write_line("fn __aot_broadcast_mul_scalar_vec<F, S: Clone, T: Clone, R>(f: F, scalar: S, values: Vec<T>) -> Vec<R>");
        self.write_line("where");
        self.indent();
        self.write_line("F: Fn(S, T) -> R + Copy,");
        self.dedent();
        self.write_line("{");
        self.indent();
        self.write_line("let mut out: Vec<R> = Vec::with_capacity(values.len());");
        self.write_line("for value in values {");
        self.indent();
        self.write_line("out.push(f(scalar.clone(), value.clone()));");
        self.dedent();
        self.write_line("}");
        self.write_line("out");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("fn __aot_broadcast_add_row_vec<F, A: Clone, B: Clone, R>(f: F, row: Vec<Vec<A>>, col: Vec<B>) -> Vec<Vec<R>>");
        self.write_line("where");
        self.indent();
        self.write_line("F: Fn(A, B) -> R + Copy,");
        self.dedent();
        self.write_line("{");
        self.indent();
        self.write_line("let width = if row.is_empty() { 0 } else { row[0].len() };");
        self.write_line("let mut out: Vec<Vec<R>> = Vec::with_capacity(col.len());");
        self.write_line("for c in col {");
        self.indent();
        self.write_line("let mut out_row: Vec<R> = Vec::with_capacity(width);");
        self.write_line("for i in 0..width {");
        self.indent();
        self.write_line("out_row.push(f(row[0][i].clone(), c.clone()));");
        self.dedent();
        self.write_line("}");
        self.write_line("out.push(out_row);");
        self.dedent();
        self.write_line("}");
        self.write_line("out");
        self.dedent();
        self.write_line("}");
        self.blank_line();

        self.write_line("fn __aot_broadcast_call_matrix_scalar_2<F, T: Clone, U: Clone, R>(f: F, matrix: Vec<Vec<T>>, scalar: U) -> Vec<Vec<R>>");
        self.write_line("where");
        self.indent();
        self.write_line("F: Fn(T, U) -> R + Copy,");
        self.dedent();
        self.write_line("{");
        self.indent();
        self.write_line("let mut out: Vec<Vec<R>> = Vec::with_capacity(matrix.len());");
        self.write_line("for row in matrix {");
        self.indent();
        self.write_line("let mut out_row: Vec<R> = Vec::with_capacity(row.len());");
        self.write_line("for value in row {");
        self.indent();
        self.write_line("out_row.push(f(value.clone(), scalar.clone()));");
        self.dedent();
        self.write_line("}");
        self.write_line("out.push(out_row);");
        self.dedent();
        self.write_line("}");
        self.write_line("out");
        self.dedent();
        self.write_line("}");
        self.blank_line();
    }

    /// Emit a struct definition
    pub(super) fn emit_struct(&mut self, s: &AotStruct) -> AotResult<()> {
        if self.config.emit_comments {
            self.write_line(&format!("// Julia struct: {}", s.name));
        }

        // Derive common traits
        if s.name == "Complex" {
            self.write_line("#[derive(Debug, Clone, Copy)]");
        } else {
            self.write_line("#[derive(Debug, Clone)]");
        }

        // Struct definition
        self.write_line(&format!("pub struct {} {{", s.name));
        self.indent();

        for (field_name, field_ty) in &s.fields {
            let rust_ty = self.type_to_rust(field_ty);
            self.write_line(&format!("pub {}: {},", field_name, rust_ty));
        }

        self.dedent();
        self.write_line("}");

        // Constructor impl
        self.blank_line();
        self.write_line(&format!("impl {} {{", s.name));
        self.indent();

        // new() constructor
        let params: Vec<_> = s
            .fields
            .iter()
            .map(|(name, ty)| format!("{}: {}", name, self.type_to_rust(ty)))
            .collect();
        self.write_line(&format!("pub fn new({}) -> Self {{", params.join(", ")));
        self.indent();
        self.write_line("Self {");
        self.indent();
        for (field_name, _) in &s.fields {
            self.write_line(&format!("{},", field_name));
        }
        self.dedent();
        self.write_line("}");
        self.dedent();
        self.write_line("}");

        self.dedent();
        self.write_line("}");

        if s.name == "Complex" {
            self.blank_line();
            self.write_line("impl std::ops::Add for Complex {");
            self.indent();
            self.write_line("type Output = Complex;");
            self.write_line("fn add(self, rhs: Complex) -> Self::Output {");
            self.indent();
            self.write_line("Complex::new(self.re + rhs.re, self.im + rhs.im)");
            self.dedent();
            self.write_line("}");
            self.dedent();
            self.write_line("}");

            self.blank_line();
            self.write_line("impl std::ops::Mul for Complex {");
            self.indent();
            self.write_line("type Output = Complex;");
            self.write_line("fn mul(self, rhs: Complex) -> Self::Output {");
            self.indent();
            self.write_line(
                "Complex::new(self.re * rhs.re - self.im * rhs.im, self.re * rhs.im + self.im * rhs.re)",
            );
            self.dedent();
            self.write_line("}");
            self.dedent();
            self.write_line("}");

            self.blank_line();
            self.write_line("fn op_add_f64_complex(x: f64, y: Complex) -> Complex {");
            self.indent();
            self.write_line("Complex::new(x + y.re, y.im)");
            self.dedent();
            self.write_line("}");

            self.blank_line();
            self.write_line("fn op_mul_complex_f64(x: Complex, y: f64) -> Complex {");
            self.indent();
            self.write_line("Complex::new(x.re * y, x.im * y)");
            self.dedent();
            self.write_line("}");
        }

        Ok(())
    }

    /// Emit an enum definition as i32 constants
    ///
    /// Julia enums (`@enum Color red green blue`) are backed by Int32.
    /// We emit them as Rust `const` values for zero-cost representation.
    pub(super) fn emit_enum(&mut self, e: &AotEnum) -> AotResult<()> {
        if self.config.emit_comments {
            self.write_line(&format!("// Julia @enum: {}", e.name));
        }

        // Type alias for the enum's backing type
        self.write_line(&format!("pub type {} = i32;", e.name));

        // Emit each member as a named constant
        for (member_name, value) in &e.members {
            self.write_line(&format!(
                "pub const {}: {} = {};",
                member_name.to_uppercase(),
                e.name,
                value
            ));
        }

        Ok(())
    }

    /// Emit a global variable
    pub(super) fn emit_global(&mut self, global: &AotGlobal) -> AotResult<()> {
        let rust_ty = self.type_to_rust(&global.ty);

        if let Some(init) = &global.init {
            let init_expr = self.emit_expr_to_string(init)?;
            self.write_line(&format!("static {}: {} = {};", global.name, rust_ty, init_expr));
        } else {
            // For uninitialized globals, use lazy_static or similar
            // TODO(Issue #3133): Use lazy_static or OnceLock for uninitialized globals
            self.write_line(&format!("// TODO: static {}: {};", global.name, rust_ty));
        }

        Ok(())
    }

    /// Find which variables (from a given set of parameter names) are reassigned in the body
    pub(super) fn find_reassigned_vars(
        &self,
        body: &[AotStmt],
        params: &[(String, StaticType)],
    ) -> HashSet<String> {
        let param_names: HashSet<_> = params.iter().map(|(name, _)| name.clone()).collect();
        let mut reassigned = HashSet::new();

        fn collect_from_stmts(
            stmts: &[AotStmt],
            param_names: &HashSet<String>,
            reassigned: &mut HashSet<String>,
        ) {
            for stmt in stmts {
                collect_from_stmt(stmt, param_names, reassigned);
            }
        }

        fn collect_from_stmt(
            stmt: &AotStmt,
            param_names: &HashSet<String>,
            reassigned: &mut HashSet<String>,
        ) {
            match stmt {
                AotStmt::Assign { target, .. } => {
                    // Check if target is a simple variable that matches a parameter
                    if let AotExpr::Var { name, .. } = target {
                        if param_names.contains(name) {
                            reassigned.insert(name.clone());
                        }
                    }
                }
                AotStmt::CompoundAssign { target, .. } => {
                    if let AotExpr::Var { name, .. } = target {
                        if param_names.contains(name) {
                            reassigned.insert(name.clone());
                        }
                    }
                }
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    collect_from_stmts(then_branch, param_names, reassigned);
                    if let Some(else_stmts) = else_branch {
                        collect_from_stmts(else_stmts, param_names, reassigned);
                    }
                }
                AotStmt::While { body, .. } => {
                    collect_from_stmts(body, param_names, reassigned);
                }
                AotStmt::ForRange { body, .. } => {
                    collect_from_stmts(body, param_names, reassigned);
                }
                AotStmt::ForEach { body, .. } => {
                    collect_from_stmts(body, param_names, reassigned);
                }
                _ => {}
            }
        }

        collect_from_stmts(body, &param_names, &mut reassigned);
        reassigned
    }

    /// Emit a function definition
    pub(super) fn emit_function(&mut self, func: &AotFunction) -> AotResult<()> {
        // Determine the function name to use
        // Use mangled name if this function has multiple dispatch methods
        let use_mangled = self.needs_dispatch(&func.name);
        let func_name = if use_mangled {
            func.mangled_name()
        } else {
            AotFunction::sanitize_function_name(&func.name)
        };

        if self.config.emit_comments {
            if func.is_generic {
                self.write_line(&format!("// Generic function: {}", func.name));
            } else if use_mangled {
                self.write_line(&format!(
                    "// Function: {} (mangled: {})",
                    func.name, func_name
                ));
            } else {
                self.write_line(&format!("// Function: {}", func.name));
            }
        }

        // Find which parameters are reassigned in the function body
        let reassigned_params = self.find_reassigned_vars(&func.body, &func.params);

        // Function signature - add mut to reassigned parameters
        let params: Vec<_> = func
            .params
            .iter()
            .map(|(name, ty)| {
                if reassigned_params.contains(name) {
                    format!("mut {}: {}", name, self.type_to_rust(ty))
                } else {
                    format!("{}: {}", name, self.type_to_rust(ty))
                }
            })
            .collect();
        let return_ty = self.type_to_rust(&func.return_type);

        self.write_line(&format!(
            "pub fn {}({}) -> {} {{",
            func_name,
            params.join(", "),
            return_ty
        ));
        self.indent();

        // Function body
        // The last statement may need special handling for implicit return
        let body_len = func.body.len();
        for (i, stmt) in func.body.iter().enumerate() {
            let is_last = i == body_len - 1;
            if is_last {
                self.emit_stmt_maybe_return(stmt, &func.return_type)?;
            } else {
                self.emit_stmt(stmt)?;
            }
        }

        self.dedent();
        self.write_line("}");

        Ok(())
    }

    /// Emit main function
    pub(super) fn emit_main(&mut self, stmts: &[AotStmt]) -> AotResult<()> {
        self.write_line("pub fn main() {");
        self.indent();

        for stmt in stmts {
            self.emit_stmt(stmt)?;
        }

        self.dedent();
        self.write_line("}");

        Ok(())
    }
}
