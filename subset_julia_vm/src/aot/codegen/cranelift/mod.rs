//! Cranelift code generation backend
//!
//! This module provides a fast JIT compilation backend using Cranelift.
//! It generates native code directly, enabling millisecond-scale compilation
//! times compared to the rustc backend.
//!
//! # Features
//!
//! - Fast compilation (milliseconds vs seconds)
//! - Direct native code generation
//! - No external compiler dependency
//!
//! # Usage
//!
//! ```ignore
//! use subset_julia_vm::aot::codegen::cranelift::CraneliftCodeGenerator;
//!
//! let mut codegen = CraneliftCodeGenerator::new()?;
//! let result = codegen.generate_module(&ir_module)?;
//! let func_ptr = codegen.get_function_ptr("my_function")?;
//! ```

mod helpers;

use super::{CodeGenerator, CodegenConfig};
use crate::aot::ir::{
    BinOpKind, ConstValue, Instruction, IrFunction, IrModule, Terminator, UnaryOpKind, VarRef,
};
use crate::aot::types::JuliaType;
use crate::aot::AotResult;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::types as cl_types;
use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, Function, InstBuilder, MemFlags, Signature, Value,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;
use target_lexicon::Triple;

use helpers::{collect_phi_info, create_signature, field_name_to_offset, julia_type_to_cranelift};

// External libm function declarations for pow and fmod
extern "C" {
    fn pow(x: f64, y: f64) -> f64;
    fn powf(x: f32, y: f32) -> f32;
    fn fmod(x: f64, y: f64) -> f64;
    fn fmodf(x: f32, y: f32) -> f32;
}

/// Error types specific to Cranelift code generation
#[derive(Debug)]
pub enum CraneliftError {
    /// Module creation failed
    ModuleCreation(String),
    /// Function compilation failed
    FunctionCompilation(String),
    /// Type conversion failed
    TypeConversion(String),
    /// Unsupported feature
    Unsupported(String),
    /// Module error
    Module(String),
}

impl std::fmt::Display for CraneliftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CraneliftError::ModuleCreation(msg) => write!(f, "Module creation error: {}", msg),
            CraneliftError::FunctionCompilation(msg) => {
                write!(f, "Function compilation error: {}", msg)
            }
            CraneliftError::TypeConversion(msg) => write!(f, "Type conversion error: {}", msg),
            CraneliftError::Unsupported(msg) => write!(f, "Unsupported feature: {}", msg),
            CraneliftError::Module(msg) => write!(f, "Module error: {}", msg),
        }
    }
}

impl std::error::Error for CraneliftError {}

/// Cranelift-based code generator
///
/// Generates native code directly using Cranelift JIT compilation.
pub struct CraneliftCodeGenerator {
    /// Configuration for code generation
    /// Retained for backend parity and future tunables even when some paths
    /// do not read configuration fields directly.
    #[allow(dead_code)]
    config: CodegenConfig,
    /// JIT module for compilation
    module: JITModule,
    /// Function builder context (reused across functions)
    builder_context: FunctionBuilderContext,
    /// Codegen context
    ctx: Context,
    /// Map of function names to their IDs
    function_ids: HashMap<String, FuncId>,
    /// Map of function names to their pointers (after finalization)
    function_ptrs: HashMap<String, *const u8>,
    /// Declared libm function IDs for pow/fmod
    libm_func_ids: HashMap<String, FuncId>,
}

/// Compilation context passed through to free compilation functions
struct CompileCtx {
    /// Map of IR function names to Cranelift FuncRefs (for calls)
    func_refs: HashMap<String, FuncRef>,
    /// Libm function refs (pow, powf, fmod, fmodf)
    libm_refs: HashMap<String, FuncRef>,
    /// For each block label: ordered list of phi destination VarRefs
    phi_params: HashMap<String, Vec<VarRef>>,
    /// For each (source_block, dest_block): ordered list of source VarRefs to pass
    phi_incoming: HashMap<(String, String), Vec<VarRef>>,
}

impl CraneliftCodeGenerator {
    /// Create a new Cranelift code generator
    pub fn new() -> Result<Self, CraneliftError> {
        Self::with_config(CodegenConfig::default())
    }

    /// Create a new Cranelift code generator with custom configuration
    pub fn with_config(config: CodegenConfig) -> Result<Self, CraneliftError> {
        // Set up the ISA (Instruction Set Architecture)
        let mut flag_builder = settings::builder();

        // Enable optimization
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))?;

        let isa_builder = cranelift_codegen::isa::lookup(Triple::host())
            .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))?;

        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))?;

        // Create JIT module
        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

        // Register libm symbols for pow and fmod
        builder.symbol("pow", pow as *const u8);
        builder.symbol("powf", powf as *const u8);
        builder.symbol("fmod", fmod as *const u8);
        builder.symbol("fmodf", fmodf as *const u8);

        let module = JITModule::new(builder);

        Ok(Self {
            config,
            module,
            builder_context: FunctionBuilderContext::new(),
            ctx: Context::new(),
            function_ids: HashMap::new(),
            function_ptrs: HashMap::new(),
            libm_func_ids: HashMap::new(),
        })
    }

    /// Declare a function in the module
    fn declare_function(&mut self, func: &IrFunction) -> Result<FuncId, CraneliftError> {
        let sig = create_signature(func)?;
        let func_id = self
            .module
            .declare_function(&func.name, Linkage::Export, &sig)
            .map_err(|e| CraneliftError::Module(e.to_string()))?;

        self.function_ids.insert(func.name.clone(), func_id);
        Ok(func_id)
    }

    /// Ensure libm functions (pow, fmod) are declared in the module
    fn ensure_libm_declared(&mut self) -> Result<(), CraneliftError> {
        let libm_sigs: [(&str, cl_types::Type); 4] = [
            ("pow", cl_types::F64),
            ("powf", cl_types::F32),
            ("fmod", cl_types::F64),
            ("fmodf", cl_types::F32),
        ];
        for (name, ty) in &libm_sigs {
            if !self.libm_func_ids.contains_key(*name) {
                let mut sig = Signature::new(CallConv::SystemV);
                sig.params.push(AbiParam::new(*ty));
                sig.params.push(AbiParam::new(*ty));
                sig.returns.push(AbiParam::new(*ty));
                let id = self
                    .module
                    .declare_function(name, Linkage::Import, &sig)
                    .map_err(|e| CraneliftError::Module(e.to_string()))?;
                self.libm_func_ids.insert(name.to_string(), id);
            }
        }
        Ok(())
    }

    /// Compile a single function
    fn compile_function(&mut self, func: &IrFunction) -> Result<(), CraneliftError> {
        let func_id = if let Some(id) = self.function_ids.get(&func.name) {
            *id
        } else {
            self.declare_function(func)?
        };

        // Ensure libm functions are declared
        self.ensure_libm_declared()?;

        let sig = create_signature(func)?;
        self.ctx.func = Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
            sig,
        );

        // Build compilation context
        let mut compile_ctx = CompileCtx {
            func_refs: HashMap::new(),
            libm_refs: HashMap::new(),
            phi_params: HashMap::new(),
            phi_incoming: HashMap::new(),
        };

        // Declare function references for all called functions
        for block in &func.blocks {
            for inst in &block.instructions {
                if let Instruction::Call { func: callee, .. } = inst {
                    if !compile_ctx.func_refs.contains_key(callee) {
                        if let Some(&callee_id) = self.function_ids.get(callee) {
                            let func_ref = self
                                .module
                                .declare_func_in_func(callee_id, &mut self.ctx.func);
                            compile_ctx.func_refs.insert(callee.clone(), func_ref);
                        }
                    }
                }
            }
        }

        // Declare libm function references in current function
        for (name, &lid) in &self.libm_func_ids {
            let func_ref = self.module.declare_func_in_func(lid, &mut self.ctx.func);
            compile_ctx.libm_refs.insert(name.clone(), func_ref);
        }

        // Collect phi node information
        collect_phi_info(func, &mut compile_ctx);

        // Compile function body
        {
            let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_context);
            compile_function_body(&mut builder, func, &compile_ctx)?;
            builder.finalize();
        }

        self.module
            .define_function(func_id, &mut self.ctx)
            .map_err(|e| CraneliftError::FunctionCompilation(e.to_string()))?;
        self.module.clear_context(&mut self.ctx);
        Ok(())
    }

    /// Finalize the module and get function pointers
    pub fn finalize(&mut self) -> Result<(), CraneliftError> {
        self.module
            .finalize_definitions()
            .map_err(|e| CraneliftError::Module(e.to_string()))?;

        // Get function pointers
        for (name, id) in &self.function_ids {
            let ptr = self.module.get_finalized_function(*id);
            self.function_ptrs.insert(name.clone(), ptr);
        }

        Ok(())
    }

    /// Get a function pointer by name
    pub fn get_function_ptr(&self, name: &str) -> Option<*const u8> {
        self.function_ptrs.get(name).copied()
    }

    /// Get a typed function pointer
    ///
    /// # Safety
    ///
    /// The caller must ensure the function signature matches the actual compiled function.
    pub unsafe fn get_typed_function<F>(&self, name: &str) -> Option<F>
    where
        F: Copy,
    {
        self.get_function_ptr(name)
            .map(|ptr| std::mem::transmute_copy(&ptr))
    }
}

// ============================================================================
// Free functions for compilation (to avoid borrow checker issues)
// ============================================================================

/// Get phi argument values for a jump from source_label to target_label
fn get_phi_args(
    var_map: &HashMap<String, Value>,
    source_label: &str,
    target_label: &str,
    compile_ctx: &CompileCtx,
) -> Result<Vec<Value>, CraneliftError> {
    let key = (source_label.to_string(), target_label.to_string());
    if let Some(vars) = compile_ctx.phi_incoming.get(&key) {
        vars.iter().map(|v| get_var(var_map, v)).collect()
    } else {
        Ok(Vec::new())
    }
}

/// Compile the function body
fn compile_function_body(
    builder: &mut FunctionBuilder,
    func: &IrFunction,
    compile_ctx: &CompileCtx,
) -> Result<(), CraneliftError> {
    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);
    builder.seal_block(entry_block);

    let mut var_map: HashMap<String, Value> = HashMap::new();
    let mut block_map: HashMap<String, Block> = HashMap::new();

    block_map.insert("entry".to_string(), entry_block);

    let block_params = builder.block_params(entry_block).to_vec();
    for (i, (name, _)) in func.params.iter().enumerate() {
        var_map.insert(name.clone(), block_params[i]);
    }

    // Create blocks with phi node parameters
    for ir_block in &func.blocks {
        if ir_block.label != "entry" {
            let block = builder.create_block();
            if let Some(phi_dests) = compile_ctx.phi_params.get(&ir_block.label) {
                for dest in phi_dests {
                    let cl_type = julia_type_to_cranelift(&dest.ty)?;
                    builder.append_block_param(block, cl_type);
                }
            }
            block_map.insert(ir_block.label.clone(), block);
        }
    }

    // Compile each block
    for ir_block in &func.blocks {
        let block = *block_map.get(&ir_block.label).ok_or_else(|| {
            CraneliftError::FunctionCompilation(format!(
                "block '{}' not found in block_map",
                ir_block.label
            ))
        })?;

        if ir_block.label != "entry" {
            builder.switch_to_block(block);
            // Map phi destinations to block parameters
            if let Some(phi_dests) = compile_ctx.phi_params.get(&ir_block.label) {
                let params = builder.block_params(block).to_vec();
                for (i, dest) in phi_dests.iter().enumerate() {
                    var_map.insert(var_key(dest), params[i]);
                }
            }
        }

        for inst in &ir_block.instructions {
            compile_instruction(builder, inst, &mut var_map, compile_ctx)?;
        }

        if let Some(term) = &ir_block.terminator {
            compile_terminator(
                builder,
                term,
                &var_map,
                &block_map,
                &ir_block.label,
                compile_ctx,
            )?;
        }

        if ir_block.label != "entry" {
            builder.seal_block(block);
        }
    }

    Ok(())
}

/// Create a unique key for a variable
fn var_key(var: &VarRef) -> String {
    if var.version == 0 {
        var.name.clone()
    } else {
        format!("{}.{}", var.name, var.version)
    }
}

/// Get a variable's value from the map
fn get_var(var_map: &HashMap<String, Value>, var: &VarRef) -> Result<Value, CraneliftError> {
    let key = var_key(var);
    var_map
        .get(&key)
        .copied()
        .ok_or_else(|| CraneliftError::FunctionCompilation(format!("Unknown variable: {}", key)))
}

/// Compile a single instruction
fn compile_instruction(
    builder: &mut FunctionBuilder,
    inst: &Instruction,
    var_map: &mut HashMap<String, Value>,
    compile_ctx: &CompileCtx,
) -> Result<(), CraneliftError> {
    match inst {
        Instruction::LoadConst { dest, value } => {
            let val = compile_const(builder, value)?;
            var_map.insert(var_key(dest), val);
        }

        Instruction::Copy { dest, src } => {
            let src_val = get_var(var_map, src)?;
            var_map.insert(var_key(dest), src_val);
        }

        Instruction::BinOp {
            dest,
            op,
            left,
            right,
        } => {
            let left_val = get_var(var_map, left)?;
            let right_val = get_var(var_map, right)?;
            let result = compile_binop(builder, *op, left_val, right_val, &dest.ty, compile_ctx)?;
            var_map.insert(var_key(dest), result);
        }

        Instruction::UnaryOp { dest, op, operand } => {
            let operand_val = get_var(var_map, operand)?;
            let result = compile_unaryop(builder, *op, operand_val, &dest.ty)?;
            var_map.insert(var_key(dest), result);
        }

        Instruction::Call { dest, func, args } => {
            if let Some(&func_ref) = compile_ctx.func_refs.get(func) {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| get_var(var_map, a))
                    .collect::<Result<_, _>>()?;
                let call_inst = builder.ins().call(func_ref, &arg_vals);
                if let Some(dest_var) = dest {
                    let results = builder.inst_results(call_inst);
                    if !results.is_empty() {
                        var_map.insert(var_key(dest_var), results[0]);
                    } else {
                        let placeholder = builder.ins().iconst(cl_types::I8, 0);
                        var_map.insert(var_key(dest_var), placeholder);
                    }
                }
            } else if let Some(dest_var) = dest {
                // Unknown function: create typed placeholder
                let cl_type = julia_type_to_cranelift(&dest_var.ty)?;
                let placeholder = match cl_type {
                    cl_types::F64 => builder.ins().f64const(0.0),
                    cl_types::F32 => builder.ins().f32const(0.0),
                    _ => builder.ins().iconst(cl_type, 0),
                };
                var_map.insert(var_key(dest_var), placeholder);
            }
        }

        Instruction::GetIndex { dest, array, index } => {
            let array_val = get_var(var_map, array)?;
            let index_val = get_var(var_map, index)?;
            let elem_type = julia_type_to_cranelift(&dest.ty)?;
            let elem_size = elem_type.bytes() as i64;
            // Julia is 1-indexed: convert to 0-based
            let one = builder.ins().iconst(cl_types::I64, 1);
            let zero_index = builder.ins().isub(index_val, one);
            let offset = builder.ins().imul_imm(zero_index, elem_size);
            let addr = builder.ins().iadd(array_val, offset);
            let result = builder.ins().load(elem_type, MemFlags::new(), addr, 0);
            var_map.insert(var_key(dest), result);
        }

        Instruction::SetIndex {
            array,
            index,
            value,
        } => {
            let array_val = get_var(var_map, array)?;
            let index_val = get_var(var_map, index)?;
            let val = get_var(var_map, value)?;
            let elem_type = julia_type_to_cranelift(&value.ty)?;
            let elem_size = elem_type.bytes() as i64;
            let one = builder.ins().iconst(cl_types::I64, 1);
            let zero_index = builder.ins().isub(index_val, one);
            let offset = builder.ins().imul_imm(zero_index, elem_size);
            let addr = builder.ins().iadd(array_val, offset);
            builder.ins().store(MemFlags::new(), val, addr, 0);
        }

        Instruction::GetField {
            dest,
            object,
            field,
        } => {
            let obj_val = get_var(var_map, object)?;
            let field_type = julia_type_to_cranelift(&dest.ty)?;
            let offset = field_name_to_offset(field);
            let result = builder
                .ins()
                .load(field_type, MemFlags::new(), obj_val, offset);
            var_map.insert(var_key(dest), result);
        }

        Instruction::SetField {
            object,
            field,
            value,
        } => {
            let obj_val = get_var(var_map, object)?;
            let val = get_var(var_map, value)?;
            let offset = field_name_to_offset(field);
            builder.ins().store(MemFlags::new(), val, obj_val, offset);
        }

        Instruction::TypeAssert { dest, src, ty: _ty } => {
            let src_val = get_var(var_map, src)?;
            var_map.insert(var_key(dest), src_val);
        }

        Instruction::Phi { dest, incoming: _ } => {
            // Phi nodes are handled via block parameters.
            // The value was mapped in compile_function_body when switching to the block.
            // If not present (e.g., entry block), use a typed placeholder.
            if !var_map.contains_key(&var_key(dest)) {
                let cl_type = julia_type_to_cranelift(&dest.ty)?;
                let placeholder = match cl_type {
                    cl_types::F64 => builder.ins().f64const(0.0),
                    cl_types::F32 => builder.ins().f32const(0.0),
                    _ => builder.ins().iconst(cl_type, 0),
                };
                var_map.insert(var_key(dest), placeholder);
            }
        }
    }

    Ok(())
}

/// Compile a constant value
fn compile_const(
    builder: &mut FunctionBuilder,
    value: &ConstValue,
) -> Result<Value, CraneliftError> {
    let val = match value {
        ConstValue::Int64(v) => builder.ins().iconst(cl_types::I64, *v),
        ConstValue::Int32(v) => builder.ins().iconst(cl_types::I32, *v as i64),
        ConstValue::Float64(v) => builder.ins().f64const(*v),
        ConstValue::Float32(v) => builder.ins().f32const(*v),
        ConstValue::Bool(v) => builder.ins().iconst(cl_types::I8, if *v { 1 } else { 0 }),
        ConstValue::Char(v) => builder.ins().iconst(cl_types::I32, *v as i64),
        ConstValue::Nothing => builder.ins().iconst(cl_types::I8, 0),
        ConstValue::String(_) => {
            // Strings need runtime support
            builder.ins().iconst(cl_types::I64, 0)
        }
    };
    Ok(val)
}

/// Compile a binary operation
fn compile_binop(
    builder: &mut FunctionBuilder,
    op: BinOpKind,
    left: Value,
    right: Value,
    result_ty: &JuliaType,
    compile_ctx: &CompileCtx,
) -> Result<Value, CraneliftError> {
    let is_float = matches!(result_ty, JuliaType::Float32 | JuliaType::Float64);

    let result = match op {
        // Arithmetic
        BinOpKind::Add => {
            if is_float {
                builder.ins().fadd(left, right)
            } else {
                builder.ins().iadd(left, right)
            }
        }
        BinOpKind::Sub => {
            if is_float {
                builder.ins().fsub(left, right)
            } else {
                builder.ins().isub(left, right)
            }
        }
        BinOpKind::Mul => {
            if is_float {
                builder.ins().fmul(left, right)
            } else {
                builder.ins().imul(left, right)
            }
        }
        BinOpKind::Div => {
            if is_float {
                builder.ins().fdiv(left, right)
            } else {
                builder.ins().sdiv(left, right)
            }
        }
        BinOpKind::Rem => {
            if is_float {
                let fname = if matches!(result_ty, JuliaType::Float32) {
                    "fmodf"
                } else {
                    "fmod"
                };
                if let Some(&fmod_ref) = compile_ctx.libm_refs.get(fname) {
                    let call = builder.ins().call(fmod_ref, &[left, right]);
                    builder.inst_results(call)[0]
                } else {
                    left
                }
            } else {
                builder.ins().srem(left, right)
            }
        }
        BinOpKind::Pow => {
            if is_float {
                let fname = if matches!(result_ty, JuliaType::Float32) {
                    "powf"
                } else {
                    "pow"
                };
                if let Some(&pow_ref) = compile_ctx.libm_refs.get(fname) {
                    let call = builder.ins().call(pow_ref, &[left, right]);
                    builder.inst_results(call)[0]
                } else {
                    left
                }
            } else {
                // Integer power: convert to f64, call pow, convert back
                if let Some(&pow_ref) = compile_ctx.libm_refs.get("pow") {
                    let left_f = builder.ins().fcvt_from_sint(cl_types::F64, left);
                    let right_f = builder.ins().fcvt_from_sint(cl_types::F64, right);
                    let call = builder.ins().call(pow_ref, &[left_f, right_f]);
                    let result_f = builder.inst_results(call)[0];
                    builder.ins().fcvt_to_sint_sat(cl_types::I64, result_f)
                } else {
                    left
                }
            }
        }

        // Comparison (returns i8 bool)
        BinOpKind::Eq => {
            if is_float {
                builder.ins().fcmp(FloatCC::Equal, left, right)
            } else {
                builder.ins().icmp(IntCC::Equal, left, right)
            }
        }
        BinOpKind::Ne => {
            if is_float {
                builder.ins().fcmp(FloatCC::NotEqual, left, right)
            } else {
                builder.ins().icmp(IntCC::NotEqual, left, right)
            }
        }
        BinOpKind::Lt => {
            if is_float {
                builder.ins().fcmp(FloatCC::LessThan, left, right)
            } else {
                builder.ins().icmp(IntCC::SignedLessThan, left, right)
            }
        }
        BinOpKind::Le => {
            if is_float {
                builder.ins().fcmp(FloatCC::LessThanOrEqual, left, right)
            } else {
                builder
                    .ins()
                    .icmp(IntCC::SignedLessThanOrEqual, left, right)
            }
        }
        BinOpKind::Gt => {
            if is_float {
                builder.ins().fcmp(FloatCC::GreaterThan, left, right)
            } else {
                builder.ins().icmp(IntCC::SignedGreaterThan, left, right)
            }
        }
        BinOpKind::Ge => {
            if is_float {
                builder.ins().fcmp(FloatCC::GreaterThanOrEqual, left, right)
            } else {
                builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, left, right)
            }
        }

        // Bitwise
        BinOpKind::BitAnd => builder.ins().band(left, right),
        BinOpKind::BitOr => builder.ins().bor(left, right),
        BinOpKind::BitXor => builder.ins().bxor(left, right),
        BinOpKind::Shl => builder.ins().ishl(left, right),
        BinOpKind::Shr => builder.ins().sshr(left, right),

        // Logical
        BinOpKind::And => builder.ins().band(left, right),
        BinOpKind::Or => builder.ins().bor(left, right),
    };

    Ok(result)
}

/// Compile a unary operation
fn compile_unaryop(
    builder: &mut FunctionBuilder,
    op: UnaryOpKind,
    operand: Value,
    result_ty: &JuliaType,
) -> Result<Value, CraneliftError> {
    let is_float = matches!(result_ty, JuliaType::Float32 | JuliaType::Float64);

    let result = match op {
        UnaryOpKind::Neg => {
            if is_float {
                builder.ins().fneg(operand)
            } else {
                builder.ins().ineg(operand)
            }
        }
        UnaryOpKind::Not => {
            // Logical not: compare with 0
            let zero = builder.ins().iconst(cl_types::I8, 0);
            builder.ins().icmp(IntCC::Equal, operand, zero)
        }
        UnaryOpKind::BitNot => builder.ins().bnot(operand),
    };

    Ok(result)
}

/// Compile a terminator instruction
fn compile_terminator(
    builder: &mut FunctionBuilder,
    term: &Terminator,
    var_map: &HashMap<String, Value>,
    block_map: &HashMap<String, Block>,
    current_block_label: &str,
    compile_ctx: &CompileCtx,
) -> Result<(), CraneliftError> {
    match term {
        Terminator::Return(None) => {
            builder.ins().return_(&[]);
        }
        Terminator::Return(Some(var)) => {
            let val = get_var(var_map, var)?;
            builder.ins().return_(&[val]);
        }
        Terminator::Jump(target) => {
            let target_block = block_map.get(target).ok_or_else(|| {
                CraneliftError::FunctionCompilation(format!("Unknown block: {}", target))
            })?;
            let phi_args = get_phi_args(var_map, current_block_label, target, compile_ctx)?;
            builder.ins().jump(*target_block, &phi_args);
        }
        Terminator::Branch {
            cond,
            then_block,
            else_block,
        } => {
            let cond_val = get_var(var_map, cond)?;
            let then_blk = block_map.get(then_block).ok_or_else(|| {
                CraneliftError::FunctionCompilation(format!("Unknown block: {}", then_block))
            })?;
            let else_blk = block_map.get(else_block).ok_or_else(|| {
                CraneliftError::FunctionCompilation(format!("Unknown block: {}", else_block))
            })?;
            let then_args = get_phi_args(var_map, current_block_label, then_block, compile_ctx)?;
            let else_args = get_phi_args(var_map, current_block_label, else_block, compile_ctx)?;
            builder
                .ins()
                .brif(cond_val, *then_blk, &then_args, *else_blk, &else_args);
        }
        Terminator::Switch {
            value,
            cases,
            default,
        } => {
            let val = get_var(var_map, value)?;
            let default_blk = block_map.get(default).ok_or_else(|| {
                CraneliftError::FunctionCompilation(format!("Unknown block: {}", default))
            })?;

            if cases.is_empty() {
                let default_args =
                    get_phi_args(var_map, current_block_label, default, compile_ctx)?;
                builder.ins().jump(*default_blk, &default_args);
            } else {
                // Implement switch as chained comparisons
                for (i, (case_val, target_label)) in cases.iter().enumerate() {
                    let target_blk = block_map.get(target_label).ok_or_else(|| {
                        CraneliftError::FunctionCompilation(format!(
                            "Unknown block: {}",
                            target_label
                        ))
                    })?;
                    let case_const = compile_const(builder, case_val)?;
                    let is_match = builder.ins().icmp(IntCC::Equal, val, case_const);
                    let target_args =
                        get_phi_args(var_map, current_block_label, target_label, compile_ctx)?;

                    if i == cases.len() - 1 {
                        // Last case: branch to target or default
                        let default_args =
                            get_phi_args(var_map, current_block_label, default, compile_ctx)?;
                        builder.ins().brif(
                            is_match,
                            *target_blk,
                            &target_args,
                            *default_blk,
                            &default_args,
                        );
                    } else {
                        // More cases: branch to target or continue checking
                        let next_block = builder.create_block();
                        builder
                            .ins()
                            .brif(is_match, *target_blk, &target_args, next_block, &[]);
                        builder.seal_block(next_block);
                        builder.switch_to_block(next_block);
                    }
                }
            }
        }
    }

    Ok(())
}

impl CodeGenerator for CraneliftCodeGenerator {
    fn target_name(&self) -> &str {
        "cranelift"
    }

    fn generate_function(&mut self, func: &IrFunction) -> AotResult<String> {
        self.compile_function(func)
            .map_err(|e| crate::aot::AotError::CodegenError(e.to_string()))?;
        Ok(format!("// Cranelift: compiled function {}", func.name))
    }

    fn generate_module(&mut self, module: &IrModule) -> AotResult<String> {
        // Declare all functions first
        for func in &module.functions {
            self.declare_function(func)
                .map_err(|e| crate::aot::AotError::CodegenError(e.to_string()))?;
        }

        // Compile all functions
        for func in &module.functions {
            self.compile_function(func)
                .map_err(|e| crate::aot::AotError::CodegenError(e.to_string()))?;
        }

        // Finalize
        self.finalize()
            .map_err(|e| crate::aot::AotError::CodegenError(e.to_string()))?;

        Ok(format!(
            "// Cranelift: compiled module {} with {} functions",
            module.name,
            module.functions.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_codegen() {
        let codegen = CraneliftCodeGenerator::new();
        assert!(codegen.is_ok());
    }

    #[test]
    fn test_type_conversion() {
        assert_eq!(
            julia_type_to_cranelift(&JuliaType::Int64).unwrap(),
            cl_types::I64
        );
        assert_eq!(
            julia_type_to_cranelift(&JuliaType::Float64).unwrap(),
            cl_types::F64
        );
        assert_eq!(
            julia_type_to_cranelift(&JuliaType::Bool).unwrap(),
            cl_types::I8
        );
    }

    #[test]
    fn test_simple_function() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Create a simple function: fn add(a: i64, b: i64) -> i64 { a + b }
        let mut func = IrFunction::new(
            "add".to_string(),
            vec![
                ("a".to_string(), JuliaType::Int64),
                ("b".to_string(), JuliaType::Int64),
            ],
            JuliaType::Int64,
        );

        // Add instruction: result = a + b
        let dest = VarRef::new("result".to_string(), JuliaType::Int64);
        let left = VarRef::new("a".to_string(), JuliaType::Int64);
        let right = VarRef::new("b".to_string(), JuliaType::Int64);

        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Add,
            left,
            right,
        });

        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        // Compile the function
        let result = codegen.compile_function(&func);
        assert!(result.is_ok());

        // Finalize and get function pointer
        codegen.finalize().unwrap();

        let ptr = codegen.get_function_ptr("add");
        assert!(ptr.is_some());

        // Test execution
        unsafe {
            let add_fn: fn(i64, i64) -> i64 = codegen.get_typed_function("add").unwrap();
            assert_eq!(add_fn(2, 3), 5);
            assert_eq!(add_fn(10, 20), 30);
            assert_eq!(add_fn(-5, 15), 10);
        }
    }

    #[test]
    fn test_function_call() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Create callee: fn double(x: i64) -> i64 { x + x }
        let mut double_fn = IrFunction::new(
            "double".to_string(),
            vec![("x".to_string(), JuliaType::Int64)],
            JuliaType::Int64,
        );
        let d = VarRef::new("d".to_string(), JuliaType::Int64);
        let x = VarRef::new("x".to_string(), JuliaType::Int64);
        double_fn
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: d.clone(),
                op: BinOpKind::Add,
                left: x.clone(),
                right: x,
            });
        double_fn
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(d)));

        // Create caller: fn call_double(a: i64) -> i64 { double(a) }
        let mut caller_fn = IrFunction::new(
            "call_double".to_string(),
            vec![("a".to_string(), JuliaType::Int64)],
            JuliaType::Int64,
        );
        let result = VarRef::new("result".to_string(), JuliaType::Int64);
        let a = VarRef::new("a".to_string(), JuliaType::Int64);
        caller_fn
            .entry_block_mut()
            .unwrap()
            .push(Instruction::Call {
                dest: Some(result.clone()),
                func: "double".to_string(),
                args: vec![a],
            });
        caller_fn
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(result)));

        // Build module with both functions
        let mut module = IrModule::new("test".to_string());
        module.add_function(double_fn);
        module.add_function(caller_fn);
        let gen_result = codegen.generate_module(&module);
        assert!(
            gen_result.is_ok(),
            "Module generation failed: {:?}",
            gen_result.err()
        );

        unsafe {
            let call_double: fn(i64) -> i64 = codegen.get_typed_function("call_double").unwrap();
            assert_eq!(call_double(5), 10);
            assert_eq!(call_double(21), 42);
        }
    }

    #[test]
    fn test_pow_float() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn power(a: f64, b: f64) -> f64 { a ^ b }
        let mut func = IrFunction::new(
            "power".to_string(),
            vec![
                ("a".to_string(), JuliaType::Float64),
                ("b".to_string(), JuliaType::Float64),
            ],
            JuliaType::Float64,
        );
        let dest = VarRef::new("result".to_string(), JuliaType::Float64);
        let left = VarRef::new("a".to_string(), JuliaType::Float64);
        let right = VarRef::new("b".to_string(), JuliaType::Float64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Pow,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        let result = codegen.compile_function(&func);
        assert!(result.is_ok());
        codegen.finalize().unwrap();

        unsafe {
            let power_fn: fn(f64, f64) -> f64 = codegen.get_typed_function("power").unwrap();
            assert!((power_fn(2.0, 3.0) - 8.0).abs() < 1e-10);
            assert!((power_fn(3.0, 2.0) - 9.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_float_remainder() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn remainder(a: f64, b: f64) -> f64 { a % b }
        let mut func = IrFunction::new(
            "remainder".to_string(),
            vec![
                ("a".to_string(), JuliaType::Float64),
                ("b".to_string(), JuliaType::Float64),
            ],
            JuliaType::Float64,
        );
        let dest = VarRef::new("result".to_string(), JuliaType::Float64);
        let left = VarRef::new("a".to_string(), JuliaType::Float64);
        let right = VarRef::new("b".to_string(), JuliaType::Float64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Rem,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        let result = codegen.compile_function(&func);
        assert!(result.is_ok());
        codegen.finalize().unwrap();

        unsafe {
            let rem_fn: fn(f64, f64) -> f64 = codegen.get_typed_function("remainder").unwrap();
            assert!((rem_fn(7.5, 2.0) - 1.5).abs() < 1e-10);
            assert!((rem_fn(10.0, 3.0) - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_phi_nodes() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn abs_val(x: i64) -> i64 {
        //   if x < 0 { result = -x } else { result = x }
        //   return result  // phi node merges the two values
        // }
        let mut func = IrFunction::new(
            "abs_val".to_string(),
            vec![("x".to_string(), JuliaType::Int64)],
            JuliaType::Int64,
        );

        let x = VarRef::new("x".to_string(), JuliaType::Int64);
        let cond = VarRef::new("cond".to_string(), JuliaType::Bool);
        let neg_x = VarRef::new("neg_x".to_string(), JuliaType::Int64);
        let result = VarRef::new("result".to_string(), JuliaType::Int64);

        // Entry block: cond = x < 0; branch cond, neg_block, pos_block
        let zero_const = VarRef::new("zero".to_string(), JuliaType::Int64);
        func.entry_block_mut()
            .unwrap()
            .push(Instruction::LoadConst {
                dest: zero_const.clone(),
                value: ConstValue::Int64(0),
            });
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: cond.clone(),
            op: BinOpKind::Lt,
            left: x.clone(),
            right: zero_const,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Branch {
                cond,
                then_block: "neg_block".to_string(),
                else_block: "pos_block".to_string(),
            });

        // neg_block: neg_x = -x; jump merge
        let mut neg_block = BasicBlock::new("neg_block".to_string());
        neg_block.push(Instruction::UnaryOp {
            dest: neg_x.clone(),
            op: UnaryOpKind::Neg,
            operand: x.clone(),
        });
        neg_block.set_terminator(Terminator::Jump("merge".to_string()));
        func.add_block(neg_block);

        // pos_block: jump merge (x is already the result)
        let mut pos_block = BasicBlock::new("pos_block".to_string());
        pos_block.set_terminator(Terminator::Jump("merge".to_string()));
        func.add_block(pos_block);

        // merge block: result = phi [neg_block: neg_x, pos_block: x]; return result
        let mut merge_block = BasicBlock::new("merge".to_string());
        merge_block.push(Instruction::Phi {
            dest: result.clone(),
            incoming: vec![
                ("neg_block".to_string(), neg_x),
                ("pos_block".to_string(), x),
            ],
        });
        merge_block.set_terminator(Terminator::Return(Some(result)));
        func.add_block(merge_block);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "Phi node compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let abs_fn: fn(i64) -> i64 = codegen.get_typed_function("abs_val").unwrap();
            assert_eq!(abs_fn(5), 5);
            assert_eq!(abs_fn(-3), 3);
            assert_eq!(abs_fn(0), 0);
        }
    }

    #[test]
    fn test_switch_terminator() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn switch_test(x: i64) -> i64 {
        //   switch x: case 1 -> ret 10, case 2 -> ret 20, default -> ret 0
        // }
        let mut func = IrFunction::new(
            "switch_test".to_string(),
            vec![("x".to_string(), JuliaType::Int64)],
            JuliaType::Int64,
        );

        let x = VarRef::new("x".to_string(), JuliaType::Int64);

        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Switch {
                value: x,
                cases: vec![
                    (ConstValue::Int64(1), "case1".to_string()),
                    (ConstValue::Int64(2), "case2".to_string()),
                ],
                default: "default".to_string(),
            });

        // case1: return 10
        let mut case1 = BasicBlock::new("case1".to_string());
        let c10 = VarRef::new("c10".to_string(), JuliaType::Int64);
        case1.push(Instruction::LoadConst {
            dest: c10.clone(),
            value: ConstValue::Int64(10),
        });
        case1.set_terminator(Terminator::Return(Some(c10)));
        func.add_block(case1);

        // case2: return 20
        let mut case2 = BasicBlock::new("case2".to_string());
        let c20 = VarRef::new("c20".to_string(), JuliaType::Int64);
        case2.push(Instruction::LoadConst {
            dest: c20.clone(),
            value: ConstValue::Int64(20),
        });
        case2.set_terminator(Terminator::Return(Some(c20)));
        func.add_block(case2);

        // default: return 0
        let mut default_blk = BasicBlock::new("default".to_string());
        let c0 = VarRef::new("c0".to_string(), JuliaType::Int64);
        default_blk.push(Instruction::LoadConst {
            dest: c0.clone(),
            value: ConstValue::Int64(0),
        });
        default_blk.set_terminator(Terminator::Return(Some(c0)));
        func.add_block(default_blk);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "Switch compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let switch_fn: fn(i64) -> i64 = codegen.get_typed_function("switch_test").unwrap();
            assert_eq!(switch_fn(1), 10);
            assert_eq!(switch_fn(2), 20);
            assert_eq!(switch_fn(3), 0);
            assert_eq!(switch_fn(99), 0);
        }
    }

    #[test]
    fn test_integer_pow() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn int_pow(a: i64, b: i64) -> i64 { a ^ b }
        let mut func = IrFunction::new(
            "int_pow".to_string(),
            vec![
                ("a".to_string(), JuliaType::Int64),
                ("b".to_string(), JuliaType::Int64),
            ],
            JuliaType::Int64,
        );
        let dest = VarRef::new("result".to_string(), JuliaType::Int64);
        let left = VarRef::new("a".to_string(), JuliaType::Int64);
        let right = VarRef::new("b".to_string(), JuliaType::Int64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Pow,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        let result = codegen.compile_function(&func);
        assert!(result.is_ok());
        codegen.finalize().unwrap();

        unsafe {
            let pow_fn: fn(i64, i64) -> i64 = codegen.get_typed_function("int_pow").unwrap();
            assert_eq!(pow_fn(2, 10), 1024);
            assert_eq!(pow_fn(3, 3), 27);
        }
    }
}
