//! High-level AoT IR to Rust code generator.
//!
//! This module implements `AotCodeGenerator` which generates Rust code
//! from the high-level AoT IR (`AotProgram`, `AotFunction`).

mod control_flow;
mod expressions;
mod operations;
mod program;
mod statements;
#[cfg(test)]
mod tests;

use super::CodegenConfig;
use crate::aot::ir::{AotExpr, AotFunction, AotProgram};
use crate::aot::types::StaticType;
use crate::aot::AotResult;
use std::collections::{HashMap, HashSet};

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
    /// Method table: function name -> list of (mangled_name, param_types)
    pub(super) method_table: HashMap<String, Vec<(String, Vec<StaticType>)>>,
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
    pub(super) fn current_indent(&self) -> String {
        self.config.indent.repeat(self.indent_level)
    }

    /// Get constant index value from an expression (for tuple indexing)
    ///
    /// Returns Some(index) if the expression is a constant integer literal,
    /// None otherwise. Used for generating Rust tuple field access syntax.
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
        ty.to_rust_type()
    }

    // ========== Program Generation ==========

    /// Generate a complete AoT program
    pub fn generate_program(&mut self, program: &AotProgram) -> AotResult<String> {
        self.output.clear();
        self.indent_level = 0;

        // Build method table for multiple dispatch
        self.build_method_table(program);

        // Emit prelude
        self.emit_prelude();

        // Emit struct definitions
        for s in &program.structs {
            self.emit_struct(s)?;
            self.blank_line();
        }

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

        // Emit function definitions (with mangled names for multidispatch)
        for func in &program.functions {
            self.emit_function(func)?;
            self.blank_line();
        }

        // Emit dispatcher functions for multiple dispatch
        self.emit_dispatchers()?;

        // Emit main function only if user didn't define one
        // If user defined main(), it becomes the entry point and we skip emit_main
        // to avoid duplicate main function definitions
        if !has_user_main {
            self.emit_main(&program.main)?;
        }

        Ok(std::mem::take(&mut self.output))
    }

    /// Generate a single function (convenience method)
    pub fn generate_function(&mut self, func: &AotFunction) -> AotResult<String> {
        self.output.clear();
        self.indent_level = 0;
        self.emit_function(func)?;
        Ok(std::mem::take(&mut self.output))
    }
}
