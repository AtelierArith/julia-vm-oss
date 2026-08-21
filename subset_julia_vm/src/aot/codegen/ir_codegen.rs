//! Low-level IR to Rust code generator.
//!
//! This module implements `RustCodeGenerator` which generates Rust code
//! from the low-level IR (`IrFunction`, `IrModule`).

use super::{CodeGenerator, CodegenConfig};
use crate::aot::ir::{
    BasicBlock, BinOpKind, ConstValue, Instruction, IrFunction, IrModule, Terminator, UnaryOpKind,
    VarRef,
};
use crate::aot::types::StaticType;
use crate::aot::AotResult;

/// Rust code generator
#[derive(Debug)]
pub struct RustCodeGenerator {
    /// Configuration
    config: CodegenConfig,
    /// Output buffer
    output: String,
    /// Current indentation level
    indent_level: usize,
}

impl RustCodeGenerator {
    /// Create a new Rust code generator
    pub fn new(config: CodegenConfig) -> Self {
        Self {
            config,
            output: String::new(),
            indent_level: 0,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(CodegenConfig::default())
    }

    /// Write a line with current indentation
    fn write_line(&mut self, line: &str) {
        for _ in 0..self.indent_level {
            self.output.push_str(&self.config.indent);
        }
        self.output.push_str(line);
        self.output.push('\n');
    }

    /// Write a blank line
    fn blank_line(&mut self) {
        self.output.push('\n');
    }

    /// Increase indentation
    fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease indentation
    fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Generate type annotation
    fn type_to_rust(&self, ty: &StaticType) -> String {
        ty.to_rust_type()
    }

    /// Generate variable reference
    fn var_to_rust(&self, var: &VarRef) -> String {
        if var.version == 0 {
            var.name.clone()
        } else {
            format!("{}_{}", var.name, var.version)
        }
    }

    /// Generate constant value
    fn const_to_rust(&self, value: &ConstValue) -> String {
        match value {
            ConstValue::Int64(v) => format!("{}i64", v),
            ConstValue::Int32(v) => format!("{}i32", v),
            ConstValue::Float64(v) => {
                if v.is_nan() {
                    "f64::NAN".to_string()
                } else if v.is_infinite() {
                    if *v > 0.0 {
                        "f64::INFINITY".to_string()
                    } else {
                        "f64::NEG_INFINITY".to_string()
                    }
                } else {
                    format!("{}f64", v)
                }
            }
            ConstValue::Float32(v) => {
                if v.is_nan() {
                    "f32::NAN".to_string()
                } else if v.is_infinite() {
                    if *v > 0.0 {
                        "f32::INFINITY".to_string()
                    } else {
                        "f32::NEG_INFINITY".to_string()
                    }
                } else {
                    format!("{}f32", v)
                }
            }
            ConstValue::Bool(v) => format!("{}", v),
            ConstValue::Char(c) => format!("'{}'", c),
            ConstValue::String(s) => format!("\"{}\"", s.escape_default()),
            ConstValue::Nothing => "()".to_string(),
        }
    }

    /// Generate binary operation
    fn binop_to_rust(&self, op: &BinOpKind) -> &str {
        match op {
            BinOpKind::Add => "+",
            BinOpKind::Sub => "-",
            BinOpKind::Mul => "*",
            BinOpKind::Div => "/",
            BinOpKind::Rem => "%",
            BinOpKind::Pow => ".pow", // Special case
            BinOpKind::Eq => "==",
            BinOpKind::Ne => "!=",
            BinOpKind::Lt => "<",
            BinOpKind::Le => "<=",
            BinOpKind::Gt => ">",
            BinOpKind::Ge => ">=",
            BinOpKind::BitAnd => "&",
            BinOpKind::BitOr => "|",
            BinOpKind::BitXor => "^",
            BinOpKind::Shl => "<<",
            BinOpKind::Shr => ">>",
            BinOpKind::And => "&&",
            BinOpKind::Or => "||",
        }
    }

    /// Generate unary operation
    fn unaryop_to_rust(&self, op: &UnaryOpKind) -> &str {
        match op {
            UnaryOpKind::Neg => "-",
            UnaryOpKind::Not => "!",
            UnaryOpKind::BitNot => "!",
        }
    }

    /// Generate instruction
    fn generate_instruction(&mut self, inst: &Instruction) {
        match inst {
            Instruction::LoadConst { dest, value } => {
                let dest_name = self.var_to_rust(dest);
                let value_str = self.const_to_rust(value);
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!("let {}: {} = {};", dest_name, ty, value_str));
            }
            Instruction::Copy { dest, src } => {
                let dest_name = self.var_to_rust(dest);
                let src_name = self.var_to_rust(src);
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!("let {}: {} = {};", dest_name, ty, src_name));
            }
            Instruction::BinOp {
                dest,
                op,
                left,
                right,
            } => {
                let dest_name = self.var_to_rust(dest);
                let left_name = self.var_to_rust(left);
                let right_name = self.var_to_rust(right);
                let ty = self.type_to_rust(&dest.ty);

                if matches!(op, BinOpKind::Pow) {
                    self.write_line(&format!(
                        "let {}: {} = {}.pow({});",
                        dest_name, ty, left_name, right_name
                    ));
                } else {
                    let op_str = self.binop_to_rust(op);
                    self.write_line(&format!(
                        "let {}: {} = {} {} {};",
                        dest_name, ty, left_name, op_str, right_name
                    ));
                }
            }
            Instruction::UnaryOp { dest, op, operand } => {
                let dest_name = self.var_to_rust(dest);
                let operand_name = self.var_to_rust(operand);
                let op_str = self.unaryop_to_rust(op);
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!(
                    "let {}: {} = {}{};",
                    dest_name, ty, op_str, operand_name
                ));
            }
            Instruction::Builtin { dest, op, args } => {
                let dest_name = self.var_to_rust(dest);
                let args = args
                    .iter()
                    .map(|arg| self.var_to_rust(arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!("let {dest_name}: {ty} = {op}({args});"));
            }
            Instruction::Rand { dest } => {
                let dest_name = self.var_to_rust(dest);
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!("let {dest_name}: {ty} = __sjulia_aot_rand();"));
            }
            Instruction::Call { dest, func, args } => {
                let args_str: Vec<_> = args.iter().map(|a| self.var_to_rust(a)).collect();
                let call = format!("{}({})", func, args_str.join(", "));
                if let Some(dest) = dest {
                    let dest_name = self.var_to_rust(dest);
                    let ty = self.type_to_rust(&dest.ty);
                    self.write_line(&format!("let {}: {} = {};", dest_name, ty, call));
                } else {
                    self.write_line(&format!("{};", call));
                }
            }
            Instruction::CallMulti { dests, func, args } => {
                let args_str: Vec<_> = args.iter().map(|a| self.var_to_rust(a)).collect();
                let dests_str: Vec<_> = dests.iter().map(|d| self.var_to_rust(d)).collect();
                self.write_line(&format!(
                    "let ({}) = {}({});",
                    dests_str.join(", "),
                    func,
                    args_str.join(", ")
                ));
            }
            Instruction::StructNew {
                dest,
                layout_id: _,
                size,
                align,
                fields,
            } => {
                let dest_name = self.var_to_rust(dest);
                self.write_line(&format!(
                    "// struct {} stack layout: size={}, align={}, fields={}",
                    dest_name,
                    size,
                    align,
                    fields.len()
                ));
            }
            Instruction::ArrayNew { dest, dims, init } => {
                let dest_name = self.var_to_rust(dest);
                self.write_line(&format!(
                    "// array {} allocation: rank={}, init={:?}",
                    dest_name,
                    dims.len(),
                    init
                ));
            }
            Instruction::ArraySlice { dest, .. } => {
                let dest_name = self.var_to_rust(dest);
                self.write_line(&format!("// array {} slice copy", dest_name));
            }
            Instruction::ArraySliceAssign { .. } => {
                self.write_line("// array slice assignment");
            }
            Instruction::UnitRangeLength { dest, start, stop } => {
                let dest_name = self.var_to_rust(dest);
                let start_name = self.var_to_rust(start);
                let stop_name = self.var_to_rust(stop);
                self.write_line(&format!(
                    "let {}: i64 = if {} < {} {{ 0 }} else {{ {} - {} + 1 }};",
                    dest_name, stop_name, start_name, stop_name, start_name
                ));
            }
            Instruction::GetIndex {
                dest,
                array,
                indices,
            } => {
                let dest_name = self.var_to_rust(dest);
                let array_name = self.var_to_rust(array);
                let index_names = indices
                    .iter()
                    .map(|index| format!("{} as usize", self.var_to_rust(index)))
                    .collect::<Vec<_>>()
                    .join("][");
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!(
                    "let {}: {} = {}[{}];",
                    dest_name, ty, array_name, index_names
                ));
            }
            Instruction::SetIndex {
                array,
                indices,
                value,
            } => {
                let array_name = self.var_to_rust(array);
                let index_names = indices
                    .iter()
                    .map(|index| format!("{} as usize", self.var_to_rust(index)))
                    .collect::<Vec<_>>()
                    .join("][");
                let value_name = self.var_to_rust(value);
                self.write_line(&format!(
                    "{}[{}] = {};",
                    array_name, index_names, value_name
                ));
            }
            Instruction::GetField {
                dest,
                object,
                field,
            } => {
                let dest_name = self.var_to_rust(dest);
                let object_name = self.var_to_rust(object);
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!(
                    "let {}: {} = {}.{};",
                    dest_name, ty, object_name, field
                ));
            }
            Instruction::GetFieldOffset {
                dest,
                object,
                layout_id: _,
                offset,
            } => {
                let dest_name = self.var_to_rust(dest);
                let object_name = self.var_to_rust(object);
                let ty = self.type_to_rust(&dest.ty);
                self.write_line(&format!(
                    "let {}: {} = /* load {} + {} */ Default::default();",
                    dest_name, ty, object_name, offset
                ));
            }
            Instruction::SetField {
                object,
                field,
                value,
            } => {
                let object_name = self.var_to_rust(object);
                let value_name = self.var_to_rust(value);
                self.write_line(&format!("{}.{} = {};", object_name, field, value_name));
            }
            Instruction::SetFieldOffset {
                object,
                offset,
                value,
            } => {
                let object_name = self.var_to_rust(object);
                let value_name = self.var_to_rust(value);
                self.write_line(&format!(
                    "/* store {} + {} */ {};",
                    object_name, offset, value_name
                ));
            }
            Instruction::TypeAssert { dest, src, ty } => {
                let dest_name = self.var_to_rust(dest);
                let src_name = self.var_to_rust(src);
                let ty_str = self.type_to_rust(ty);
                if self.config.runtime_checks {
                    self.write_line(&format!(
                        "let {}: {} = {}; // type assert",
                        dest_name, ty_str, src_name
                    ));
                } else {
                    self.write_line(&format!("let {}: {} = {};", dest_name, ty_str, src_name));
                }
            }
            Instruction::Phi { dest, incoming } => {
                // Phi nodes should be lowered before codegen
                if self.config.emit_comments {
                    let dest_name = self.var_to_rust(dest);
                    let sources: Vec<_> = incoming
                        .iter()
                        .map(|(label, var)| format!("{}: {}", label, self.var_to_rust(var)))
                        .collect();
                    self.write_line(&format!("// phi {} = [{}]", dest_name, sources.join(", ")));
                }
            }
        }
    }

    /// Generate terminator
    fn generate_terminator(&mut self, term: &Terminator) {
        match term {
            Terminator::Return(Some(var)) => {
                let var_name = self.var_to_rust(var);
                self.write_line(&format!("return {};", var_name));
            }
            Terminator::Return(None) => {
                self.write_line("return;");
            }
            Terminator::ReturnMany(vars) => {
                let vars_str: Vec<_> = vars.iter().map(|var| self.var_to_rust(var)).collect();
                self.write_line(&format!("return ({});", vars_str.join(", ")));
            }
            Terminator::Jump(label) => {
                self.write_line(&format!("// goto {}", label));
            }
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                let cond_name = self.var_to_rust(cond);
                self.write_line(&format!("if {} {{", cond_name));
                self.indent();
                self.write_line(&format!("// goto {}", then_block));
                self.dedent();
                self.write_line("} else {");
                self.indent();
                self.write_line(&format!("// goto {}", else_block));
                self.dedent();
                self.write_line("}");
            }
            Terminator::Switch {
                value,
                cases,
                default,
            } => {
                let value_name = self.var_to_rust(value);
                self.write_line(&format!("match {} {{", value_name));
                self.indent();
                for (case_val, label) in cases {
                    let val_str = self.const_to_rust(case_val);
                    self.write_line(&format!("{} => {{ /* goto {} */ }}", val_str, label));
                }
                self.write_line(&format!("_ => {{ /* goto {} */ }}", default));
                self.dedent();
                self.write_line("}");
            }
        }
    }

    /// Generate a basic block
    fn generate_block(&mut self, block: &BasicBlock) {
        if self.config.emit_comments {
            self.write_line(&format!("// block: {}", block.label));
        }

        for inst in &block.instructions {
            self.generate_instruction(inst);
        }

        if let Some(term) = &block.terminator {
            self.generate_terminator(term);
        }
    }
}

impl CodeGenerator for RustCodeGenerator {
    fn target_name(&self) -> &str {
        "Rust"
    }

    fn generate_function(&mut self, func: &IrFunction) -> AotResult<String> {
        self.output.clear();
        self.indent_level = 0;

        // Function signature
        let params: Vec<_> = func
            .params
            .iter()
            .map(|(name, ty)| format!("{}: {}", name, self.type_to_rust(ty)))
            .collect();
        let return_type = self.type_to_rust(&func.return_type);

        self.write_line(&format!(
            "fn {}({}) -> {} {{",
            func.name,
            params.join(", "),
            return_type
        ));
        self.indent();

        // Generate blocks
        for block in &func.blocks {
            self.generate_block(block);
        }

        self.dedent();
        self.write_line("}");

        Ok(std::mem::take(&mut self.output))
    }

    fn generate_module(&mut self, module: &IrModule) -> AotResult<String> {
        self.output.clear();
        self.indent_level = 0;

        // Module header
        self.write_line("//! Auto-generated by SubsetJuliaVM AoT compiler");
        self.write_line("//! Do not edit manually");
        self.blank_line();
        self.write_line("use subset_julia_vm_runtime::prelude::*;");
        self.blank_line();

        // Save header
        let header = std::mem::take(&mut self.output);

        // Generate functions
        let mut functions_code = String::new();
        for func in &module.functions {
            let func_code = self.generate_function(func)?;
            functions_code.push_str(&func_code);
            functions_code.push('\n');
        }

        // Combine header and functions
        self.output = header;
        self.output.push_str(&functions_code);

        Ok(std::mem::take(&mut self.output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::codegen::CodeGenerator;
    use crate::aot::ir::{IrFunction, IrModule, Terminator, VarRef};
    use crate::aot::types::StaticType;

    #[test]
    fn test_rust_codegen_simple_function() {
        let mut codegen = RustCodeGenerator::default_config();

        let mut func = IrFunction::new(
            "add_one".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );

        // Add return terminator
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(VarRef::new(
                "x".to_string(),
                StaticType::I64,
            ))));

        let result = codegen.generate_function(&func).unwrap();
        assert!(result.contains("fn add_one(x: i64) -> i64"));
        assert!(result.contains("return x;"));
    }

    #[test]
    fn test_rust_codegen_module() {
        let mut codegen = RustCodeGenerator::default_config();

        let mut module = IrModule::new("test".to_string());
        let mut func = IrFunction::new("main".to_string(), vec![], StaticType::Nothing);
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(None));
        module.add_function(func);

        let result = codegen.generate_module(&module).unwrap();
        assert!(result.contains("Auto-generated"));
        assert!(result.contains("use subset_julia_vm_runtime::prelude::*;"));
        assert!(result.contains("fn main() -> ()"));
    }

    #[test]
    fn rust_codegen_rand_uses_runtime_rng() {
        let mut codegen = RustCodeGenerator::default_config();
        let mut function = IrFunction::new("sample".to_string(), vec![], StaticType::F64);
        let destination = VarRef::new("sampled".to_string(), StaticType::F64);
        function.entry_block_mut().unwrap().push(Instruction::Rand {
            dest: destination.clone(),
        });
        function
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(destination)));

        let generated = codegen.generate_function(&function).unwrap();

        assert!(generated.contains("let sampled: f64 = __sjulia_aot_rand();"));
        assert!(!generated.contains("Default::default()"));
    }
}
