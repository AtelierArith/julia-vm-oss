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
use crate::aot::optimizer::OptLevel;
use crate::aot::types::StaticType;
use crate::aot::{AotError, AotResult, UnsupportedInstructionDiagnostic};

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::types as cl_types;
use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, Function, GlobalValue, InstBuilder, MemFlags, Signature, SourceLoc,
    StackSlotData, StackSlotKind, TrapCode, Type, Value,
};
use cranelift_codegen::isa::{CallConv, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule, ObjectProduct};
use std::collections::HashMap;
use std::path::Path;
use target_lexicon::Triple;

use helpers::{collect_phi_info, create_signature, field_name_to_offset, static_type_to_cranelift};

// External libm function declarations for scalar math builtins.
extern "C" {
    fn pow(x: f64, y: f64) -> f64;
    fn powf(x: f32, y: f32) -> f32;
    fn fmod(x: f64, y: f64) -> f64;
    fn fmodf(x: f32, y: f32) -> f32;
    fn sqrt(x: f64) -> f64;
    fn sqrtf(x: f32) -> f32;
    fn sin(x: f64) -> f64;
    fn sinf(x: f32) -> f32;
    fn cos(x: f64) -> f64;
    fn cosf(x: f32) -> f32;
    fn exp(x: f64) -> f64;
    fn expf(x: f32) -> f32;
    fn log(x: f64) -> f64;
    fn logf(x: f32) -> f32;
    fn fabs(x: f64) -> f64;
    fn fabsf(x: f32) -> f32;
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

impl CraneliftError {
    fn into_aot_error(self) -> AotError {
        match self {
            CraneliftError::Unsupported(message) => AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!("{message} (Issue #7129)"))
                    .with_workaround(
                        "use `--backend rust` for the full current AoT surface, or keep Cranelift input within the documented scalar subset",
                    ),
            ),
            other => AotError::CodegenError(other.to_string()),
        }
    }
}

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
    /// Declared libm function IDs for scalar math builtins
    libm_func_ids: HashMap<String, FuncId>,
    /// Read-only string literal payloads, keyed by their Julia string contents.
    string_data_ids: HashMap<String, DataId>,
}

/// Cranelift object-file code generator.
///
/// This mirrors the current JIT backend but writes a relocatable object through
/// `cranelift-object::ObjectModule` instead of finalizing in-process function
/// pointers.
pub struct CraneliftObjectCodeGenerator {
    /// Configuration for code generation.
    #[allow(dead_code)]
    config: CodegenConfig,
    /// Object module for relocatable output.
    module: ObjectModule,
    /// Function builder context (reused across functions).
    builder_context: FunctionBuilderContext,
    /// Codegen context.
    ctx: Context,
    /// Map of function names to their IDs.
    function_ids: HashMap<String, FuncId>,
    /// Declared libm function IDs for scalar math builtins.
    libm_func_ids: HashMap<String, FuncId>,
    /// Read-only string literal payloads, keyed by their Julia string contents.
    string_data_ids: HashMap<String, DataId>,
}

/// Compilation context passed through to free compilation functions
struct CompileCtx {
    /// Map of IR function names to Cranelift FuncRefs (for calls)
    func_refs: HashMap<String, FuncRef>,
    /// Libm function refs for scalar math builtins
    libm_refs: HashMap<String, FuncRef>,
    /// For each block label: ordered list of phi destination VarRefs
    phi_params: HashMap<String, Vec<VarRef>>,
    /// For each (source_block, dest_block): ordered list of source VarRefs to pass
    phi_incoming: HashMap<(String, String), Vec<VarRef>>,
    /// Target pointer type used for stack addresses.
    pointer_type: Type,
    /// Data-section string payload symbols available in this function.
    string_data_refs: HashMap<String, GlobalValue>,
}

impl CraneliftCodeGenerator {
    /// Create a new Cranelift code generator
    pub fn new() -> Result<Self, CraneliftError> {
        Self::with_config(CodegenConfig::default())
    }

    /// Create a new Cranelift code generator with custom configuration
    pub fn with_config(config: CodegenConfig) -> Result<Self, CraneliftError> {
        // Create JIT module
        let isa = cranelift_isa_for_config(config.opt_level, Triple::host())?;
        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

        // Register libm symbols for scalar math builtins
        builder.symbol("pow", pow as *const u8);
        builder.symbol("powf", powf as *const u8);
        builder.symbol("fmod", fmod as *const u8);
        builder.symbol("fmodf", fmodf as *const u8);
        builder.symbol("sqrt", sqrt as *const u8);
        builder.symbol("sqrtf", sqrtf as *const u8);
        builder.symbol("sin", sin as *const u8);
        builder.symbol("sinf", sinf as *const u8);
        builder.symbol("cos", cos as *const u8);
        builder.symbol("cosf", cosf as *const u8);
        builder.symbol("exp", exp as *const u8);
        builder.symbol("expf", expf as *const u8);
        builder.symbol("log", log as *const u8);
        builder.symbol("logf", logf as *const u8);
        builder.symbol("fabs", fabs as *const u8);
        builder.symbol("fabsf", fabsf as *const u8);

        let module = JITModule::new(builder);

        Ok(Self {
            config,
            module,
            builder_context: FunctionBuilderContext::new(),
            ctx: Context::new(),
            function_ids: HashMap::new(),
            function_ptrs: HashMap::new(),
            libm_func_ids: HashMap::new(),
            string_data_ids: HashMap::new(),
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

    /// Ensure libm functions are declared in the module
    fn ensure_libm_declared(&mut self) -> Result<(), CraneliftError> {
        ensure_libm_declared_in(&mut self.module, &mut self.libm_func_ids)
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
        ensure_string_data_declared_in(&mut self.module, &mut self.string_data_ids, func)?;

        compile_declared_function_in_module(
            &mut self.module,
            &mut self.ctx,
            &mut self.builder_context,
            &self.function_ids,
            &self.libm_func_ids,
            &self.string_data_ids,
            func_id,
            func,
        )
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

impl CraneliftObjectCodeGenerator {
    /// Create a new Cranelift object generator.
    pub fn new() -> Result<Self, CraneliftError> {
        Self::with_config(CodegenConfig::default())
    }

    /// Create a new Cranelift object generator with custom configuration.
    pub fn with_config(config: CodegenConfig) -> Result<Self, CraneliftError> {
        Self::with_config_and_target(config, Triple::host())
    }

    /// Create a new Cranelift object generator for an explicit target triple.
    pub fn with_config_and_target(
        config: CodegenConfig,
        target: Triple,
    ) -> Result<Self, CraneliftError> {
        let isa = cranelift_isa_for_config(config.opt_level, target)?;
        let builder = ObjectBuilder::new(
            isa,
            "juliars_cranelift",
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))?;
        let module = ObjectModule::new(builder);

        Ok(Self {
            config,
            module,
            builder_context: FunctionBuilderContext::new(),
            ctx: Context::new(),
            function_ids: HashMap::new(),
            libm_func_ids: HashMap::new(),
            string_data_ids: HashMap::new(),
        })
    }

    /// Declare a function in the object module.
    fn declare_function(&mut self, func: &IrFunction) -> Result<FuncId, CraneliftError> {
        let sig = create_signature(func)?;
        let func_id = self
            .module
            .declare_function(&func.name, Linkage::Export, &sig)
            .map_err(|e| CraneliftError::Module(e.to_string()))?;

        self.function_ids.insert(func.name.clone(), func_id);
        Ok(func_id)
    }

    /// Ensure libm functions are declared as imports in the object module.
    fn ensure_libm_declared(&mut self) -> Result<(), CraneliftError> {
        ensure_libm_declared_in(&mut self.module, &mut self.libm_func_ids)
    }

    /// Compile a single function into the object module.
    fn compile_function(&mut self, func: &IrFunction) -> Result<(), CraneliftError> {
        let func_id = if let Some(id) = self.function_ids.get(&func.name) {
            *id
        } else {
            self.declare_function(func)?
        };

        self.ensure_libm_declared()?;
        ensure_string_data_declared_in(&mut self.module, &mut self.string_data_ids, func)?;
        compile_declared_function_in_module(
            &mut self.module,
            &mut self.ctx,
            &mut self.builder_context,
            &self.function_ids,
            &self.libm_func_ids,
            &self.string_data_ids,
            func_id,
            func,
        )
    }

    /// Compile an IR module and return relocatable object bytes.
    pub fn generate_object(mut self, module: &IrModule) -> Result<Vec<u8>, CraneliftError> {
        for func in &module.functions {
            self.declare_function(func)?;
        }

        for func in &module.functions {
            self.compile_function(func)?;
        }

        let mut product = self.module.finish();
        if self.config.debug_info {
            append_cranelift_dwarf_sections(&mut product, module, &self.config)?;
        }
        product
            .emit()
            .map_err(|e| CraneliftError::Module(e.to_string()))
    }
}

fn append_cranelift_dwarf_sections(
    product: &mut ObjectProduct,
    module: &IrModule,
    config: &CodegenConfig,
) -> Result<(), CraneliftError> {
    let line_entries = cranelift_debug_line_entries(module);
    if line_entries.is_empty() {
        return Ok(());
    }

    let source_file = dwarf_source_file(&config.source_name);
    let comp_dir = dwarf_comp_dir(&config.source_name);
    let encoding = gimli::Encoding {
        format: gimli::Format::Dwarf32,
        version: 5,
        address_size: 8,
    };
    let mut line_program = gimli::write::LineProgram::new(
        encoding,
        gimli::LineEncoding::default(),
        gimli::write::LineString::String(comp_dir.as_bytes().to_vec()),
        gimli::write::LineString::String(source_file.as_bytes().to_vec()),
        None,
    );
    let file_id = line_program.add_file(
        gimli::write::LineString::String(source_file.as_bytes().to_vec()),
        line_program.default_directory(),
        None,
    );

    line_program.begin_sequence(Some(gimli::write::Address::Constant(0)));
    for (address_offset, (_, line)) in line_entries.iter().enumerate() {
        let row = line_program.row();
        row.file = file_id;
        row.line = u64::from(*line);
        row.column = 1;
        row.address_offset = address_offset as u64;
        row.is_statement = true;
        line_program.generate_row();
    }
    line_program.end_sequence(line_entries.len() as u64);

    let mut unit = gimli::write::Unit::new(encoding, line_program);
    let root = unit.root();
    {
        let root_entry = unit.get_mut(root);
        root_entry.set(
            gimli::DW_AT_name,
            gimli::write::AttributeValue::String(source_file.as_bytes().to_vec()),
        );
        root_entry.set(
            gimli::DW_AT_comp_dir,
            gimli::write::AttributeValue::String(comp_dir.as_bytes().to_vec()),
        );
        root_entry.set(
            gimli::DW_AT_producer,
            gimli::write::AttributeValue::String(b"SubsetJuliaVM Cranelift".to_vec()),
        );
        root_entry.set(
            gimli::DW_AT_language,
            gimli::write::AttributeValue::Language(gimli::DW_LANG_Julia),
        );
        root_entry.set(
            gimli::DW_AT_stmt_list,
            gimli::write::AttributeValue::LineProgramRef,
        );
    }

    for (function_name, line) in line_entries {
        let child = unit.add(root, gimli::DW_TAG_subprogram);
        let child_entry = unit.get_mut(child);
        child_entry.set(
            gimli::DW_AT_name,
            gimli::write::AttributeValue::String(function_name.into_bytes()),
        );
        child_entry.set(
            gimli::DW_AT_decl_file,
            gimli::write::AttributeValue::FileIndex(Some(file_id)),
        );
        child_entry.set(
            gimli::DW_AT_decl_line,
            gimli::write::AttributeValue::Udata(u64::from(line)),
        );
    }

    let mut dwarf = gimli::write::Dwarf::new();
    dwarf.units.add(unit);
    let mut sections =
        gimli::write::Sections::new(gimli::write::EndianVec::new(gimli::LittleEndian));
    dwarf
        .write(&mut sections)
        .map_err(|e| CraneliftError::Module(format!("failed to write DWARF debug info: {e}")))?;

    sections
        .for_each(|section_id, data| -> Result<(), CraneliftError> {
            let bytes = data.slice();
            if !bytes.is_empty() {
                let object_section = product.object.add_section(
                    Vec::new(),
                    section_id.name().as_bytes().to_vec(),
                    object::SectionKind::Debug,
                );
                product.object.append_section_data(object_section, bytes, 1);
            }
            Ok(())
        })
        .map_err(|err| err)
}

fn cranelift_debug_line_entries(module: &IrModule) -> Vec<(String, u32)> {
    module
        .functions
        .iter()
        .filter_map(|func| func.debug_line.map(|line| (func.name.clone(), line)))
        .collect()
}

fn dwarf_source_file(source_name: &str) -> String {
    let file_name = Path::new(source_name)
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .filter(|file| !file.is_empty())
        .unwrap_or_else(|| source_name.to_string());
    sanitize_dwarf_string(&file_name, "<unknown>")
}

fn dwarf_comp_dir(source_name: &str) -> String {
    let comp_dir = Path::new(source_name)
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
        .filter(|dir| !dir.is_empty())
        .unwrap_or_else(|| ".".to_string());
    sanitize_dwarf_string(&comp_dir, ".")
}

fn sanitize_dwarf_string(value: &str, fallback: &str) -> String {
    if value.is_empty() || value.as_bytes().contains(&0) {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn cranelift_isa_for_config(
    opt_level: OptLevel,
    target: Triple,
) -> Result<OwnedTargetIsa, CraneliftError> {
    let mut flag_builder = settings::builder();

    let cranelift_opt_level = cranelift_opt_level_setting(opt_level);
    flag_builder
        .set("opt_level", cranelift_opt_level)
        .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))?;
    flag_builder
        .set("enable_llvm_abi_extensions", "true")
        .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))?;

    let isa_builder = cranelift_codegen::isa::lookup(target)
        .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))?;

    isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| CraneliftError::ModuleCreation(e.to_string()))
}

fn cranelift_opt_level_setting(opt_level: OptLevel) -> &'static str {
    match opt_level {
        OptLevel::O0 => "none",
        OptLevel::O1 | OptLevel::O2 => "speed",
        OptLevel::O3 => "speed_and_size",
    }
}

// ============================================================================
// Free functions for compilation (to avoid borrow checker issues)
// ============================================================================

fn libm_signatures() -> [(&'static str, cl_types::Type, usize); 16] {
    [
        ("pow", cl_types::F64, 2),
        ("powf", cl_types::F32, 2),
        ("fmod", cl_types::F64, 2),
        ("fmodf", cl_types::F32, 2),
        ("sqrt", cl_types::F64, 1),
        ("sqrtf", cl_types::F32, 1),
        ("sin", cl_types::F64, 1),
        ("sinf", cl_types::F32, 1),
        ("cos", cl_types::F64, 1),
        ("cosf", cl_types::F32, 1),
        ("exp", cl_types::F64, 1),
        ("expf", cl_types::F32, 1),
        ("log", cl_types::F64, 1),
        ("logf", cl_types::F32, 1),
        ("fabs", cl_types::F64, 1),
        ("fabsf", cl_types::F32, 1),
    ]
}

fn ensure_libm_declared_in<M: Module>(
    module: &mut M,
    libm_func_ids: &mut HashMap<String, FuncId>,
) -> Result<(), CraneliftError> {
    for (name, ty, arity) in libm_signatures() {
        if !libm_func_ids.contains_key(name) {
            let mut sig = Signature::new(CallConv::SystemV);
            for _ in 0..arity {
                sig.params.push(AbiParam::new(ty));
            }
            sig.returns.push(AbiParam::new(ty));
            let id = module
                .declare_function(name, Linkage::Import, &sig)
                .map_err(|e| CraneliftError::Module(e.to_string()))?;
            libm_func_ids.insert(name.to_string(), id);
        }
    }
    Ok(())
}

fn string_literal_payload(value: &str) -> Result<Vec<u8>, CraneliftError> {
    let len = u64::try_from(value.len()).map_err(|_| {
        CraneliftError::Unsupported(
            "Cranelift backend cannot lower String constants larger than u64::MAX bytes"
                .to_string(),
        )
    })?;
    let mut payload = Vec::with_capacity(8 + value.len() + 1);
    payload.extend_from_slice(&len.to_le_bytes());
    payload.extend_from_slice(value.as_bytes());
    payload.push(0);
    Ok(payload)
}

fn collect_string_literals(func: &IrFunction) -> Vec<String> {
    let mut values = Vec::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::LoadConst {
                value: ConstValue::String(value),
                ..
            } = inst
            {
                if !values.contains(value) {
                    values.push(value.clone());
                }
            }
        }
    }
    values
}

fn ensure_string_data_declared_in<M: Module>(
    module: &mut M,
    string_data_ids: &mut HashMap<String, DataId>,
    func: &IrFunction,
) -> Result<(), CraneliftError> {
    for value in collect_string_literals(func) {
        if string_data_ids.contains_key(&value) {
            continue;
        }

        let symbol = format!("__sjulia_str_{}", string_data_ids.len());
        let data_id = module
            .declare_data(&symbol, Linkage::Local, false, false)
            .map_err(|e| CraneliftError::Module(e.to_string()))?;
        let mut data = DataDescription::new();
        data.define(string_literal_payload(&value)?.into_boxed_slice());
        data.set_align(8);
        module
            .define_data(data_id, &data)
            .map_err(|e| CraneliftError::Module(e.to_string()))?;
        string_data_ids.insert(value, data_id);
    }
    Ok(())
}

fn compile_declared_function_in_module<M: Module>(
    module: &mut M,
    ctx: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    function_ids: &HashMap<String, FuncId>,
    libm_func_ids: &HashMap<String, FuncId>,
    string_data_ids: &HashMap<String, DataId>,
    func_id: FuncId,
    func: &IrFunction,
) -> Result<(), CraneliftError> {
    let sig = create_signature(func)?;
    ctx.func = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
        sig,
    );

    let mut compile_ctx = CompileCtx {
        func_refs: HashMap::new(),
        libm_refs: HashMap::new(),
        phi_params: HashMap::new(),
        phi_incoming: HashMap::new(),
        pointer_type: module.target_config().pointer_type(),
        string_data_refs: HashMap::new(),
    };

    for block in &func.blocks {
        for inst in &block.instructions {
            if let Instruction::Call { func: callee, .. }
            | Instruction::CallMulti { func: callee, .. } = inst
            {
                if !compile_ctx.func_refs.contains_key(callee) {
                    if let Some(&callee_id) = function_ids.get(callee) {
                        let func_ref = module.declare_func_in_func(callee_id, &mut ctx.func);
                        compile_ctx.func_refs.insert(callee.clone(), func_ref);
                    }
                }
            }
        }
    }

    for (name, &lid) in libm_func_ids {
        let func_ref = module.declare_func_in_func(lid, &mut ctx.func);
        compile_ctx.libm_refs.insert(name.clone(), func_ref);
    }

    for value in collect_string_literals(func) {
        let Some(&data_id) = string_data_ids.get(&value) else {
            return Err(CraneliftError::FunctionCompilation(format!(
                "String constant data was not declared for `{}`",
                func.name
            )));
        };
        let global = module.declare_data_in_func(data_id, &mut ctx.func);
        compile_ctx.string_data_refs.insert(value, global);
    }

    collect_phi_info(func, &mut compile_ctx);

    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        compile_function_body(&mut builder, func, &compile_ctx)?;
        builder.finalize();
    }

    ctx.verify(module.isa()).map_err(|e| {
        CraneliftError::FunctionCompilation(format!(
            "Cranelift verifier failed before compile for `{}`:\n{}\n{}",
            func.name,
            e,
            ctx.func.display()
        ))
    })?;

    module.define_function(func_id, ctx).map_err(|e| {
        CraneliftError::FunctionCompilation(format!("{}\n{}", e, ctx.func.display()))
    })?;
    module.clear_context(ctx);
    Ok(())
}

/// Get phi argument values for a jump from source_label to target_label
fn get_phi_args(
    var_map: &HashMap<String, Value>,
    source_label: &str,
    target_label: &str,
    compile_ctx: &CompileCtx,
) -> Result<Vec<Value>, CraneliftError> {
    let key = (source_label.to_string(), target_label.to_string());
    if let Some(vars) = compile_ctx.phi_incoming.get(&key) {
        let phi_count = compile_ctx.phi_params.get(target_label).map_or(0, Vec::len);
        if vars.len() != phi_count {
            return Err(CraneliftError::FunctionCompilation(format!(
                "phi edge {} -> {} has {} incoming value(s), expected {}",
                source_label,
                target_label,
                vars.len(),
                phi_count
            )));
        }
        vars.iter().map(|v| get_var(var_map, v)).collect()
    } else if compile_ctx
        .phi_params
        .get(target_label)
        .is_some_and(|params| !params.is_empty())
    {
        Err(CraneliftError::FunctionCompilation(format!(
            "missing phi incoming values for edge {} -> {}",
            source_label, target_label
        )))
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
    if let Some(line) = func.debug_line {
        builder.set_srcloc(SourceLoc::new(line));
    }

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
                    let cl_type = static_type_to_cranelift(&dest.ty)?;
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

fn is_switch_key_type_supported(ty: &StaticType) -> bool {
    matches!(
        ty,
        StaticType::I8
            | StaticType::I16
            | StaticType::I32
            | StaticType::I64
            | StaticType::I128
            | StaticType::U8
            | StaticType::U16
            | StaticType::U32
            | StaticType::U64
            | StaticType::U128
            | StaticType::Bool
            | StaticType::Char
    )
}

fn is_unsigned_integer_type(ty: &StaticType) -> bool {
    matches!(
        ty,
        StaticType::U8 | StaticType::U16 | StaticType::U32 | StaticType::U64 | StaticType::U128
    )
}

fn is_cranelift_float_type(ty: &StaticType) -> bool {
    matches!(ty, StaticType::F16 | StaticType::F32 | StaticType::F64)
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
            let val = compile_const(builder, value, compile_ctx)?;
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
            let result = compile_binop(
                builder,
                *op,
                left_val,
                &left.ty,
                right_val,
                &right.ty,
                &dest.ty,
                compile_ctx,
            )?;
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
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| get_var(var_map, a))
                    .collect::<Result<_, _>>()?;
                if let Some(result) =
                    compile_math_call(builder, func, &arg_vals, args, &dest_var.ty, compile_ctx)?
                {
                    var_map.insert(var_key(dest_var), result);
                } else {
                    return Err(CraneliftError::Unsupported(format!(
                        "Cranelift backend does not yet lower runtime-checked call `{}` for `{}` (Issue #7111)",
                        func,
                        var_key(dest_var)
                    )));
                }
            } else {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend does not yet lower runtime-checked call `{}` (Issue #7111)",
                    func
                )));
            }
        }

        Instruction::CallMulti { dests, func, args } => {
            let Some(&func_ref) = compile_ctx.func_refs.get(func) else {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend does not yet lower runtime-checked multi-return call `{}` (Issue #7117)",
                    func
                )));
            };
            let arg_vals: Vec<Value> = args
                .iter()
                .map(|a| get_var(var_map, a))
                .collect::<Result<_, _>>()?;
            let call_inst = builder.ins().call(func_ref, &arg_vals);
            let results = builder.inst_results(call_inst);
            if results.len() != dests.len() {
                return Err(CraneliftError::FunctionCompilation(format!(
                    "multi-return call `{}` produced {} result(s), expected {}",
                    func,
                    results.len(),
                    dests.len()
                )));
            }
            for (dest, result) in dests.iter().zip(results.iter().copied()) {
                var_map.insert(var_key(dest), result);
            }
        }

        Instruction::StructNew {
            dest,
            size,
            align,
            fields,
        } => {
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (*size).max(1),
                align.ilog2() as u8,
            ));
            let addr = builder.ins().stack_addr(compile_ctx.pointer_type, slot, 0);
            for field in fields {
                let val = get_var(var_map, &field.value)?;
                builder
                    .ins()
                    .store(MemFlags::new(), val, addr, field.offset);
            }
            var_map.insert(var_key(dest), addr);
        }

        Instruction::GetIndex { .. } => {
            return Err(CraneliftError::Unsupported(
                "Cranelift backend does not yet lower array indexing without bounds metadata (Issue #7109)".to_string(),
            ));
        }

        Instruction::SetIndex { .. } => {
            return Err(CraneliftError::Unsupported(
                "Cranelift backend does not yet lower array mutation without bounds metadata (Issue #7109)".to_string(),
            ));
        }

        Instruction::GetField {
            dest,
            object,
            field,
        } => {
            let obj_val = get_var(var_map, object)?;
            let field_type = static_type_to_cranelift(&dest.ty)?;
            let offset = field_name_to_offset(field);
            let result = builder
                .ins()
                .load(field_type, MemFlags::new(), obj_val, offset);
            var_map.insert(var_key(dest), result);
        }
        Instruction::GetFieldOffset {
            dest,
            object,
            offset,
        } => {
            let obj_val = get_var(var_map, object)?;
            let field_type = static_type_to_cranelift(&dest.ty)?;
            let result = builder
                .ins()
                .load(field_type, MemFlags::new(), obj_val, *offset);
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
        Instruction::SetFieldOffset {
            object,
            offset,
            value,
        } => {
            let obj_val = get_var(var_map, object)?;
            let val = get_var(var_map, value)?;
            builder.ins().store(MemFlags::new(), val, obj_val, *offset);
        }

        Instruction::TypeAssert { dest, src, ty } => {
            if &src.ty != ty || &dest.ty != ty {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend does not yet lower runtime type assertion/conversion from `{}` to `{}` with Julia conversion semantics (Issue #7111; Issue #7123)",
                    src.ty.julia_type_name(),
                    ty.julia_type_name()
                )));
            }
            let src_val = get_var(var_map, src)?;
            var_map.insert(var_key(dest), src_val);
        }

        Instruction::Phi { dest, incoming: _ } => {
            // Phi nodes are handled via block parameters.
            // The value was mapped in compile_function_body when switching to the block.
            if !var_map.contains_key(&var_key(dest)) {
                return Err(CraneliftError::FunctionCompilation(format!(
                    "phi destination {} is missing its block parameter mapping",
                    dest
                )));
            }
        }
    }

    Ok(())
}

/// Compile a constant value
fn compile_const(
    builder: &mut FunctionBuilder,
    value: &ConstValue,
    compile_ctx: &CompileCtx,
) -> Result<Value, CraneliftError> {
    let val = match value {
        ConstValue::Int64(v) => builder.ins().iconst(cl_types::I64, *v),
        ConstValue::Int32(v) => builder.ins().iconst(cl_types::I32, *v as i64),
        ConstValue::Float64(v) => builder.ins().f64const(*v),
        ConstValue::Float32(v) => builder.ins().f32const(*v),
        ConstValue::Bool(v) => builder.ins().iconst(cl_types::I8, if *v { 1 } else { 0 }),
        ConstValue::Char(v) => builder.ins().iconst(cl_types::I32, *v as i64),
        ConstValue::Nothing => builder.ins().iconst(cl_types::I8, 0),
        ConstValue::String(v) => {
            let Some(&global) = compile_ctx.string_data_refs.get(v) else {
                return Err(CraneliftError::FunctionCompilation(
                    "String constant data was not declared in the current Cranelift function"
                        .to_string(),
                ));
            };
            builder.ins().symbol_value(compile_ctx.pointer_type, global)
        }
    };
    Ok(val)
}

fn compile_math_call(
    builder: &mut FunctionBuilder,
    func: &str,
    arg_vals: &[Value],
    args: &[VarRef],
    result_ty: &StaticType,
    compile_ctx: &CompileCtx,
) -> Result<Option<Value>, CraneliftError> {
    match func {
        "sqrt" | "sin" | "cos" | "exp" | "log" => {
            if arg_vals.len() != 1 {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend expected one argument for math builtin `{}`",
                    func
                )));
            }
            let Some(symbol) = unary_libm_symbol(func, result_ty) else {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend does not yet lower `{}` returning `{}` through libm",
                    func,
                    result_ty.julia_type_name()
                )));
            };
            let arg = coerce_numeric_to_float(builder, arg_vals[0], &args[0].ty, result_ty)?;
            Ok(Some(call_libm(builder, compile_ctx, symbol, &[arg])?))
        }
        "abs" => {
            if arg_vals.len() != 1 {
                return Err(CraneliftError::Unsupported(
                    "Cranelift backend expected one argument for math builtin `abs`".to_string(),
                ));
            }
            Ok(Some(compile_abs_call(
                builder,
                arg_vals[0],
                &args[0].ty,
                result_ty,
                compile_ctx,
            )?))
        }
        "__sjulia_string_length" => {
            if arg_vals.len() != 1 || args.len() != 1 {
                return Err(CraneliftError::Unsupported(
                    "Cranelift backend expected one argument for String length".to_string(),
                ));
            }
            if args[0].ty != StaticType::Str || *result_ty != StaticType::I64 {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend String length expected String -> Int64, got {} -> {}",
                    args[0].ty.julia_type_name(),
                    result_ty.julia_type_name()
                )));
            }
            Ok(Some(builder.ins().load(
                cl_types::I64,
                MemFlags::new(),
                arg_vals[0],
                0,
            )))
        }
        _ => Ok(None),
    }
}

fn unary_libm_symbol(func: &str, result_ty: &StaticType) -> Option<&'static str> {
    match (func, result_ty) {
        ("sqrt", StaticType::F64) => Some("sqrt"),
        ("sqrt", StaticType::F16) => Some("sqrtf"),
        ("sqrt", StaticType::F32) => Some("sqrtf"),
        ("sin", StaticType::F64) => Some("sin"),
        ("sin", StaticType::F16) => Some("sinf"),
        ("sin", StaticType::F32) => Some("sinf"),
        ("cos", StaticType::F64) => Some("cos"),
        ("cos", StaticType::F16) => Some("cosf"),
        ("cos", StaticType::F32) => Some("cosf"),
        ("exp", StaticType::F64) => Some("exp"),
        ("exp", StaticType::F16) => Some("expf"),
        ("exp", StaticType::F32) => Some("expf"),
        ("log", StaticType::F64) => Some("log"),
        ("log", StaticType::F16) => Some("logf"),
        ("log", StaticType::F32) => Some("logf"),
        _ => None,
    }
}

fn compile_abs_call(
    builder: &mut FunctionBuilder,
    arg: Value,
    arg_ty: &StaticType,
    result_ty: &StaticType,
    compile_ctx: &CompileCtx,
) -> Result<Value, CraneliftError> {
    match result_ty {
        StaticType::F64 => {
            let arg = coerce_numeric_to_float(builder, arg, arg_ty, result_ty)?;
            call_libm(builder, compile_ctx, "fabs", &[arg])
        }
        StaticType::F16 | StaticType::F32 => {
            let arg = coerce_numeric_to_float(builder, arg, arg_ty, result_ty)?;
            call_libm(builder, compile_ctx, "fabsf", &[arg])
        }
        ty if ty.is_unsigned() || matches!(ty, StaticType::Bool) => Ok(arg),
        ty if ty.is_signed() => {
            if arg_ty != result_ty {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend does not yet lower integer `abs` from `{}` to `{}`",
                    arg_ty.julia_type_name(),
                    result_ty.julia_type_name()
                )));
            }
            let cl_ty = static_type_to_cranelift(result_ty)?;
            let zero = builder.ins().iconst(cl_ty, 0);
            let is_negative = builder.ins().icmp(IntCC::SignedLessThan, arg, zero);
            let negated = builder.ins().ineg(arg);
            Ok(builder.ins().select(is_negative, negated, arg))
        }
        _ => Err(CraneliftError::Unsupported(format!(
            "Cranelift backend does not yet lower `abs` for `{}`",
            result_ty.julia_type_name()
        ))),
    }
}

fn call_libm(
    builder: &mut FunctionBuilder,
    compile_ctx: &CompileCtx,
    symbol: &str,
    args: &[Value],
) -> Result<Value, CraneliftError> {
    let Some(&func_ref) = compile_ctx.libm_refs.get(symbol) else {
        return Err(CraneliftError::FunctionCompilation(format!(
            "libm function `{}` was not declared in the current Cranelift function",
            symbol
        )));
    };
    let call = builder.ins().call(func_ref, args);
    Ok(builder.inst_results(call)[0])
}

fn coerce_numeric_to_float(
    builder: &mut FunctionBuilder,
    value: Value,
    from_ty: &StaticType,
    target_ty: &StaticType,
) -> Result<Value, CraneliftError> {
    if from_ty == target_ty {
        return Ok(value);
    }

    let target = match target_ty {
        StaticType::F16 | StaticType::F32 | StaticType::F64 => static_type_to_cranelift(target_ty)?,
        _ => {
            return Err(CraneliftError::Unsupported(format!(
                "Cranelift backend expected floating-point libm result, got `{}`",
                target_ty.julia_type_name()
            )));
        }
    };

    match from_ty {
        StaticType::F32 if matches!(target_ty, StaticType::F64) => {
            Ok(builder.ins().fpromote(target, value))
        }
        StaticType::F64 if matches!(target_ty, StaticType::F16 | StaticType::F32) => {
            Ok(builder.ins().fdemote(target, value))
        }
        ty if ty.is_signed() => Ok(builder.ins().fcvt_from_sint(target, value)),
        ty if ty.is_unsigned() || matches!(ty, StaticType::Bool) => {
            Ok(builder.ins().fcvt_from_uint(target, value))
        }
        _ => Err(CraneliftError::Unsupported(format!(
            "Cranelift backend does not yet coerce `{}` to `{}` for libm",
            from_ty.julia_type_name(),
            target_ty.julia_type_name()
        ))),
    }
}

/// Compile a binary operation
fn compile_binop(
    builder: &mut FunctionBuilder,
    op: BinOpKind,
    left: Value,
    left_ty: &StaticType,
    right: Value,
    right_ty: &StaticType,
    result_ty: &StaticType,
    compile_ctx: &CompileCtx,
) -> Result<Value, CraneliftError> {
    let op_ty = if binop_is_comparison(op) {
        comparison_operand_type(left_ty, right_ty)
    } else {
        result_ty
    };
    let is_float = is_cranelift_float_type(op_ty);
    let (left, right) =
        coerce_binop_operands(builder, op, left, left_ty, right, right_ty, result_ty)?;

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
            } else if is_unsigned_integer_type(result_ty) {
                let cl_ty = static_type_to_cranelift(result_ty)?;
                let zero = builder.ins().iconst(cl_ty, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, right, zero);
                builder
                    .ins()
                    .trapnz(is_zero, TrapCode::INTEGER_DIVISION_BY_ZERO);
                builder.ins().udiv(left, right)
            } else {
                let cl_ty = static_type_to_cranelift(result_ty)?;
                let zero = builder.ins().iconst(cl_ty, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, right, zero);
                builder
                    .ins()
                    .trapnz(is_zero, TrapCode::INTEGER_DIVISION_BY_ZERO);
                builder.ins().sdiv(left, right)
            }
        }
        BinOpKind::Rem => {
            if is_float {
                let fname = if matches!(result_ty, StaticType::F16 | StaticType::F32) {
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
            } else if is_unsigned_integer_type(result_ty) {
                let cl_ty = static_type_to_cranelift(result_ty)?;
                let zero = builder.ins().iconst(cl_ty, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, right, zero);
                builder
                    .ins()
                    .trapnz(is_zero, TrapCode::INTEGER_DIVISION_BY_ZERO);
                builder.ins().urem(left, right)
            } else {
                let cl_ty = static_type_to_cranelift(result_ty)?;
                let zero = builder.ins().iconst(cl_ty, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, right, zero);
                builder
                    .ins()
                    .trapnz(is_zero, TrapCode::INTEGER_DIVISION_BY_ZERO);
                builder.ins().srem(left, right)
            }
        }
        BinOpKind::Pow => {
            if is_float {
                let fname = if matches!(result_ty, StaticType::F16 | StaticType::F32) {
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
            let pred = if is_float {
                builder.ins().fcmp(FloatCC::Equal, left, right)
            } else {
                builder.ins().icmp(IntCC::Equal, left, right)
            };
            bool_to_i8(builder, pred)
        }
        BinOpKind::Ne => {
            let pred = if is_float {
                builder.ins().fcmp(FloatCC::NotEqual, left, right)
            } else {
                builder.ins().icmp(IntCC::NotEqual, left, right)
            };
            bool_to_i8(builder, pred)
        }
        BinOpKind::Lt => {
            let pred = if is_float {
                builder.ins().fcmp(FloatCC::LessThan, left, right)
            } else {
                builder.ins().icmp(IntCC::SignedLessThan, left, right)
            };
            bool_to_i8(builder, pred)
        }
        BinOpKind::Le => {
            let pred = if is_float {
                builder.ins().fcmp(FloatCC::LessThanOrEqual, left, right)
            } else {
                builder
                    .ins()
                    .icmp(IntCC::SignedLessThanOrEqual, left, right)
            };
            bool_to_i8(builder, pred)
        }
        BinOpKind::Gt => {
            let pred = if is_float {
                builder.ins().fcmp(FloatCC::GreaterThan, left, right)
            } else {
                builder.ins().icmp(IntCC::SignedGreaterThan, left, right)
            };
            bool_to_i8(builder, pred)
        }
        BinOpKind::Ge => {
            let pred = if is_float {
                builder.ins().fcmp(FloatCC::GreaterThanOrEqual, left, right)
            } else {
                builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, left, right)
            };
            bool_to_i8(builder, pred)
        }

        // Bitwise
        BinOpKind::BitAnd => builder.ins().band(left, right),
        BinOpKind::BitOr => builder.ins().bor(left, right),
        BinOpKind::BitXor => builder.ins().bxor(left, right),
        BinOpKind::Shl => builder.ins().ishl(left, right),
        BinOpKind::Shr => {
            if is_unsigned_integer_type(result_ty) {
                builder.ins().ushr(left, right)
            } else {
                builder.ins().sshr(left, right)
            }
        }

        // Logical
        BinOpKind::And => builder.ins().band(left, right),
        BinOpKind::Or => builder.ins().bor(left, right),
    };

    Ok(result)
}

fn bool_to_i8(builder: &mut FunctionBuilder, predicate: Value) -> Value {
    let mask = builder.ins().bmask(cl_types::I8, predicate);
    let one = builder.ins().iconst(cl_types::I8, 1);
    builder.ins().band(mask, one)
}

fn coerce_binop_operands(
    builder: &mut FunctionBuilder,
    op: BinOpKind,
    left: Value,
    left_ty: &StaticType,
    right: Value,
    right_ty: &StaticType,
    result_ty: &StaticType,
) -> Result<(Value, Value), CraneliftError> {
    if matches!(op, BinOpKind::And | BinOpKind::Or) {
        return Ok((left, right));
    }

    if matches!(op, BinOpKind::Shl | BinOpKind::Shr) {
        return Ok((
            coerce_bool_operand(builder, left, left_ty, result_ty)?,
            coerce_shift_amount(builder, right, right_ty, result_ty)?,
        ));
    }

    let target_ty = if binop_is_comparison(op) {
        comparison_operand_type(left_ty, right_ty)
    } else {
        result_ty
    };

    Ok((
        coerce_bool_operand(builder, left, left_ty, target_ty)?,
        coerce_bool_operand(builder, right, right_ty, target_ty)?,
    ))
}

fn coerce_shift_amount(
    builder: &mut FunctionBuilder,
    value: Value,
    from_ty: &StaticType,
    target_ty: &StaticType,
) -> Result<Value, CraneliftError> {
    if !from_ty.is_integer() && !matches!(from_ty, StaticType::Bool | StaticType::Char) {
        return Err(CraneliftError::Unsupported(format!(
            "Cranelift backend does not yet lower shift amount type `{}`",
            from_ty.julia_type_name()
        )));
    }

    let source = static_type_to_cranelift(from_ty)?;
    let target = static_type_to_cranelift(target_ty)?;
    if !target.is_int() {
        return Err(CraneliftError::Unsupported(format!(
            "Cranelift backend does not yet lower shift result type `{}`",
            target_ty.julia_type_name()
        )));
    }

    if source == target {
        Ok(value)
    } else if source.bytes() < target.bytes() {
        if from_ty.is_signed() {
            Ok(builder.ins().sextend(target, value))
        } else {
            Ok(builder.ins().uextend(target, value))
        }
    } else {
        Ok(builder.ins().ireduce(target, value))
    }
}

fn binop_is_comparison(op: BinOpKind) -> bool {
    matches!(
        op,
        BinOpKind::Eq
            | BinOpKind::Ne
            | BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Gt
            | BinOpKind::Ge
    )
}

fn comparison_operand_type<'a>(
    left_ty: &'a StaticType,
    right_ty: &'a StaticType,
) -> &'a StaticType {
    match (left_ty, right_ty) {
        (StaticType::Bool, other) if other != &StaticType::Bool => other,
        (other, StaticType::Bool) if other != &StaticType::Bool => other,
        _ => left_ty,
    }
}

fn coerce_bool_operand(
    builder: &mut FunctionBuilder,
    value: Value,
    from_ty: &StaticType,
    target_ty: &StaticType,
) -> Result<Value, CraneliftError> {
    if from_ty != &StaticType::Bool || target_ty == &StaticType::Bool {
        return Ok(value);
    }

    let target = static_type_to_cranelift(target_ty)?;
    if target.is_float() {
        return Ok(builder.ins().fcvt_from_uint(target, value));
    }

    if target.bytes() <= cl_types::I8.bytes() {
        Ok(value)
    } else {
        Ok(builder.ins().uextend(target, value))
    }
}

/// Compile a unary operation
fn compile_unaryop(
    builder: &mut FunctionBuilder,
    op: UnaryOpKind,
    operand: Value,
    result_ty: &StaticType,
) -> Result<Value, CraneliftError> {
    let is_float = is_cranelift_float_type(result_ty);

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
        Terminator::ReturnMany(vars) => {
            let vals = vars
                .iter()
                .map(|var| get_var(var_map, var))
                .collect::<Result<Vec<_>, _>>()?;
            builder.ins().return_(&vals);
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
            if !is_switch_key_type_supported(&value.ty) {
                return Err(CraneliftError::Unsupported(format!(
                    "Cranelift backend does not yet lower switch on `{}` values (Issue #7114)",
                    value.ty.julia_type_name()
                )));
            }
            let switch_ty = static_type_to_cranelift(&value.ty)?;
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
                    let case_ty = static_type_to_cranelift(&case_val.get_type())?;
                    if case_ty != switch_ty {
                        return Err(CraneliftError::FunctionCompilation(format!(
                            "switch case type {} does not match switch value type {}",
                            case_val.get_type().julia_type_name(),
                            value.ty.julia_type_name()
                        )));
                    }
                    let case_const = compile_const(builder, case_val, compile_ctx)?;
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
            .map_err(CraneliftError::into_aot_error)?;
        Ok(format!("// Cranelift: compiled function {}", func.name))
    }

    fn generate_module(&mut self, module: &IrModule) -> AotResult<String> {
        // Declare all functions first
        for func in &module.functions {
            self.declare_function(func)
                .map_err(CraneliftError::into_aot_error)?;
        }

        // Compile all functions
        for func in &module.functions {
            self.compile_function(func)
                .map_err(CraneliftError::into_aot_error)?;
        }

        // Finalize
        self.finalize().map_err(CraneliftError::into_aot_error)?;

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
    fn cranelift_opt_level_maps_to_settings_issue_7091() {
        assert_eq!(cranelift_opt_level_setting(OptLevel::O0), "none");
        assert_eq!(cranelift_opt_level_setting(OptLevel::O1), "speed");
        assert_eq!(cranelift_opt_level_setting(OptLevel::O2), "speed");
        assert_eq!(cranelift_opt_level_setting(OptLevel::O3), "speed_and_size");
    }

    #[test]
    fn cranelift_verifier_runs_before_compile_issue_7125() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new("bad_return".to_string(), vec![], StaticType::I64);
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(None));

        let err = codegen.compile_function(&func).unwrap_err();

        assert!(matches!(err, CraneliftError::FunctionCompilation(_)));
        assert!(err
            .to_string()
            .contains("Cranelift verifier failed before compile for `bad_return`"));
    }

    #[test]
    fn cranelift_object_module_emits_object_issue_7082() {
        let mut module = IrModule::new("object_smoke".to_string());
        let mut func = IrFunction::new(
            "object_add_one".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let x = VarRef::new("x".to_string(), StaticType::I64);
        let one = VarRef::new("one".to_string(), StaticType::I64);
        let result = VarRef::new("result".to_string(), StaticType::I64);
        func.entry_block_mut()
            .unwrap()
            .push(Instruction::LoadConst {
                dest: one.clone(),
                value: ConstValue::Int64(1),
            });
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: result.clone(),
            op: BinOpKind::Add,
            left: x,
            right: one,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(result)));
        module.add_function(func);

        let object_bytes = CraneliftObjectCodeGenerator::new()
            .unwrap()
            .generate_object(&module)
            .unwrap();

        assert!(!object_bytes.is_empty());
        assert!(
            object_bytes
                .windows(b"object_add_one".len())
                .any(|window| window == b"object_add_one"),
            "object output should retain the exported function symbol"
        );
    }

    #[test]
    fn cranelift_object_module_accepts_explicit_host_target_issue_7087() {
        let mut module = IrModule::new("object_target_smoke".to_string());
        let mut func = IrFunction::new(
            "target_identity".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let x = VarRef::new("x".to_string(), StaticType::I64);
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(x)));
        module.add_function(func);

        let object_bytes = CraneliftObjectCodeGenerator::with_config_and_target(
            CodegenConfig::default(),
            Triple::host(),
        )
        .unwrap()
        .generate_object(&module)
        .unwrap();

        assert!(!object_bytes.is_empty());
        assert!(object_bytes
            .windows(b"target_identity".len())
            .any(|window| window == b"target_identity"));
    }

    #[test]
    fn cranelift_object_emits_elf_macho_and_coff_issue_7088() {
        fn identity_module(function_name: &str) -> IrModule {
            let mut module = IrModule::new(format!("{function_name}_module"));
            let mut func = IrFunction::new(
                function_name.to_string(),
                vec![("x".to_string(), StaticType::I64)],
                StaticType::I64,
            );
            let x = VarRef::new("x".to_string(), StaticType::I64);
            func.entry_block_mut()
                .unwrap()
                .set_terminator(Terminator::Return(Some(x)));
            module.add_function(func);
            module
        }

        let cases: [(&str, &str, &[u8]); 3] = [
            ("x86_64-unknown-linux-gnu", "elf_identity", b"\x7fELF"),
            (
                "x86_64-apple-darwin",
                "macho_identity",
                &[0xcf, 0xfa, 0xed, 0xfe],
            ),
            ("x86_64-pc-windows-msvc", "coff_identity", &[0x64, 0x86]),
        ];

        for (triple, function_name, magic) in cases {
            let target = triple.parse::<Triple>().unwrap();
            let module = identity_module(function_name);
            let object_bytes = CraneliftObjectCodeGenerator::with_config_and_target(
                CodegenConfig::default(),
                target,
            )
            .unwrap_or_else(|err| panic!("failed to create ObjectModule for {triple}: {err}"))
            .generate_object(&module)
            .unwrap_or_else(|err| panic!("failed to emit object for {triple}: {err}"));

            assert!(
                object_bytes.starts_with(magic),
                "object for {triple} did not start with expected format magic"
            );
            assert!(object_bytes
                .windows(function_name.len())
                .any(|window| window == function_name.as_bytes()));
        }
    }

    #[test]
    fn cranelift_unsupported_maps_to_aot_diagnostic_issue_7129() {
        let err =
            CraneliftError::Unsupported("runtime Value boundary".to_string()).into_aot_error();

        let AotError::UnsupportedInstruction(diagnostic) = err else {
            panic!("expected unsupported diagnostic");
        };
        assert!(diagnostic.message.contains("runtime Value boundary"));
        assert!(diagnostic.message.contains("Issue #7129"));
        assert!(diagnostic.workaround.is_some());
    }

    #[test]
    fn test_type_conversion() {
        assert_eq!(
            static_type_to_cranelift(&StaticType::I64).unwrap(),
            cl_types::I64
        );
        assert_eq!(
            static_type_to_cranelift(&StaticType::I128).unwrap(),
            cl_types::I128
        );
        assert_eq!(
            static_type_to_cranelift(&StaticType::U128).unwrap(),
            cl_types::I128
        );
        assert_eq!(
            static_type_to_cranelift(&StaticType::F16).unwrap(),
            cl_types::F32
        );
        assert_eq!(
            static_type_to_cranelift(&StaticType::F64).unwrap(),
            cl_types::F64
        );
        assert_eq!(
            static_type_to_cranelift(&StaticType::Bool).unwrap(),
            cl_types::I8
        );
    }

    #[test]
    fn test_runtime_value_type_requires_rooting_contract() {
        let err = static_type_to_cranelift(&StaticType::Any).unwrap_err();

        assert!(matches!(err, CraneliftError::Unsupported(_)));
        assert!(err.to_string().contains("rooting/safepoint contract"));
    }

    #[test]
    fn cranelift_signature_rejects_runtime_value_boundary_issue_6947() {
        let param_func = IrFunction::new(
            "boxed_param".to_string(),
            vec![("x".to_string(), StaticType::Any)],
            StaticType::I64,
        );
        let err = create_signature(&param_func).unwrap_err();
        assert!(matches!(err, CraneliftError::Unsupported(_)));
        assert!(err.to_string().contains("rooting/safepoint contract"));

        let return_func = IrFunction::new(
            "boxed_return".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::Any,
        );
        let err = create_signature(&return_func).unwrap_err();
        assert!(matches!(err, CraneliftError::Unsupported(_)));
        assert!(err.to_string().contains("rooting/safepoint contract"));
    }

    #[test]
    fn cranelift_i128_u128_scalar_ops_compile_issue_7092() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut iadd_func = IrFunction::new(
            "i128_add".to_string(),
            vec![
                ("x".to_string(), StaticType::I128),
                ("y".to_string(), StaticType::I128),
            ],
            StaticType::I128,
        );
        let x = VarRef::new("x".to_string(), StaticType::I128);
        let y = VarRef::new("y".to_string(), StaticType::I128);
        let iadd_result = VarRef::new("result".to_string(), StaticType::I128);
        iadd_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: iadd_result.clone(),
                op: BinOpKind::Add,
                left: x,
                right: y,
            });
        iadd_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(iadd_result)));
        codegen.compile_function(&iadd_func).unwrap();

        let mut ushr_func = IrFunction::new(
            "u128_shr".to_string(),
            vec![
                ("x".to_string(), StaticType::U128),
                ("amount".to_string(), StaticType::U128),
            ],
            StaticType::U128,
        );
        let ux = VarRef::new("x".to_string(), StaticType::U128);
        let amount = VarRef::new("amount".to_string(), StaticType::U128);
        let ushr_result = VarRef::new("result".to_string(), StaticType::U128);
        ushr_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: ushr_result.clone(),
                op: BinOpKind::Shr,
                left: ux,
                right: amount,
            });
        ushr_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(ushr_result)));
        codegen.compile_function(&ushr_func).unwrap();

        codegen.finalize().unwrap();

        unsafe {
            let i128_add: fn(i128, i128) -> i128 = codegen.get_typed_function("i128_add").unwrap();
            let u128_shr: fn(u128, u128) -> u128 = codegen.get_typed_function("u128_shr").unwrap();

            assert_eq!(i128_add(i128::MAX, 1), i128::MIN);
            assert_eq!(u128_shr(u128::MAX, 4), u128::MAX >> 4);
        }
    }

    #[test]
    fn cranelift_f16_uses_f32_carrier_issue_7093() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut add_func = IrFunction::new(
            "f16_add".to_string(),
            vec![
                ("x".to_string(), StaticType::F16),
                ("y".to_string(), StaticType::F16),
            ],
            StaticType::F16,
        );
        let x = VarRef::new("x".to_string(), StaticType::F16);
        let y = VarRef::new("y".to_string(), StaticType::F16);
        let result = VarRef::new("result".to_string(), StaticType::F16);
        add_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: result.clone(),
                op: BinOpKind::Add,
                left: x,
                right: y,
            });
        add_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(result)));
        codegen.compile_function(&add_func).unwrap();

        let mut sqrt_func = IrFunction::new(
            "f16_sqrt".to_string(),
            vec![("x".to_string(), StaticType::F16)],
            StaticType::F16,
        );
        let sqrt_arg = VarRef::new("x".to_string(), StaticType::F16);
        let sqrt_result = VarRef::new("result".to_string(), StaticType::F16);
        sqrt_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::Call {
                dest: Some(sqrt_result.clone()),
                func: "sqrt".to_string(),
                args: vec![sqrt_arg],
            });
        sqrt_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(sqrt_result)));
        codegen.compile_function(&sqrt_func).unwrap();

        codegen.finalize().unwrap();

        unsafe {
            let f16_add: fn(f32, f32) -> f32 = codegen.get_typed_function("f16_add").unwrap();
            let f16_sqrt: fn(f32) -> f32 = codegen.get_typed_function("f16_sqrt").unwrap();

            assert_eq!(f16_add(1.25, 2.5), 3.75);
            assert_eq!(f16_sqrt(9.0), 3.0);
        }
    }

    #[test]
    fn cranelift_module_rejects_runtime_value_return_issue_6947() {
        let mut module = IrModule::new("boxed".to_string());
        module.add_function(IrFunction::new(
            "boxed_return".to_string(),
            vec![],
            StaticType::Any,
        ));

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let err = codegen.generate_module(&module).unwrap_err();
        assert!(err.to_string().contains("rooting/safepoint contract"));
    }

    #[test]
    fn cranelift_string_constant_lowers_to_readonly_payload_issue_7094() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new("string_const".to_string(), vec![], StaticType::I64);
        let dest = VarRef::new("s".to_string(), StaticType::Str);
        let len = VarRef::new("len".to_string(), StaticType::I64);
        func.entry_block_mut()
            .unwrap()
            .push(Instruction::LoadConst {
                dest: dest.clone(),
                value: ConstValue::String("hello".to_string()),
            });
        func.entry_block_mut().unwrap().push(Instruction::Call {
            dest: Some(len.clone()),
            func: "__sjulia_string_length".to_string(),
            args: vec![dest],
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(len)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();
        unsafe {
            let string_const: fn() -> i64 = codegen.get_typed_function("string_const").unwrap();
            assert_eq!(string_const(), 5);
        }

        let mut module = IrModule::new("string_payload".to_string());
        module.add_function(func);
        let object_bytes = CraneliftObjectCodeGenerator::new()
            .unwrap()
            .generate_object(&module)
            .unwrap();
        let expected_payload = string_literal_payload("hello").unwrap();
        assert!(object_bytes
            .windows(expected_payload.len())
            .any(|window| window == expected_payload));
    }

    #[test]
    fn cranelift_getindex_requires_bounds_metadata_issue_7109() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "unchecked_getindex".to_string(),
            vec![
                ("ptr".to_string(), StaticType::I64),
                ("idx".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let array = VarRef::new("ptr".to_string(), StaticType::I64);
        let index = VarRef::new("idx".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::GetIndex {
            dest: dest.clone(),
            array,
            index,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        let err = codegen.compile_function(&func).unwrap_err();
        assert!(matches!(err, CraneliftError::Unsupported(_)));
        assert!(err.to_string().contains("bounds metadata"));
        assert!(err.to_string().contains("Issue #7109"));
    }

    #[test]
    fn cranelift_setindex_requires_bounds_metadata_issue_7109() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "unchecked_setindex".to_string(),
            vec![
                ("ptr".to_string(), StaticType::I64),
                ("idx".to_string(), StaticType::I64),
                ("value".to_string(), StaticType::I64),
            ],
            StaticType::Nothing,
        );
        let array = VarRef::new("ptr".to_string(), StaticType::I64);
        let index = VarRef::new("idx".to_string(), StaticType::I64);
        let value = VarRef::new("value".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::SetIndex {
            array,
            index,
            value,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(None));

        let err = codegen.compile_function(&func).unwrap_err();
        assert!(matches!(err, CraneliftError::Unsupported(_)));
        assert!(err.to_string().contains("bounds metadata"));
        assert!(err.to_string().contains("Issue #7109"));
    }

    #[test]
    fn cranelift_unknown_runtime_checked_call_is_gated_issue_7111() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "unknown_call".to_string(),
            vec![("x".to_string(), StaticType::F64)],
            StaticType::F64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::F64);
        let arg = VarRef::new("x".to_string(), StaticType::F64);
        func.entry_block_mut().unwrap().push(Instruction::Call {
            dest: Some(dest.clone()),
            func: "runtime_checked_unknown".to_string(),
            args: vec![arg],
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        let err = codegen.compile_function(&func).unwrap_err();
        assert!(matches!(err, CraneliftError::Unsupported(_)));
        assert!(err.to_string().contains("runtime_checked_unknown"));
        assert!(err.to_string().contains("Issue #7111"));
    }

    #[test]
    fn cranelift_libm_unary_math_builtins_issue_7122() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        for builtin in ["sqrt", "sin", "cos", "exp", "log"] {
            let mut func = IrFunction::new(
                format!("{builtin}_call"),
                vec![("x".to_string(), StaticType::F64)],
                StaticType::F64,
            );
            let dest = VarRef::new("result".to_string(), StaticType::F64);
            let arg = VarRef::new("x".to_string(), StaticType::F64);
            func.entry_block_mut().unwrap().push(Instruction::Call {
                dest: Some(dest.clone()),
                func: builtin.to_string(),
                args: vec![arg],
            });
            func.entry_block_mut()
                .unwrap()
                .set_terminator(Terminator::Return(Some(dest)));
            codegen.compile_function(&func).unwrap();
        }

        let mut sqrtf_func = IrFunction::new(
            "sqrtf_call".to_string(),
            vec![("x".to_string(), StaticType::F32)],
            StaticType::F32,
        );
        let sqrtf_dest = VarRef::new("result".to_string(), StaticType::F32);
        let sqrtf_arg = VarRef::new("x".to_string(), StaticType::F32);
        sqrtf_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::Call {
                dest: Some(sqrtf_dest.clone()),
                func: "sqrt".to_string(),
                args: vec![sqrtf_arg],
            });
        sqrtf_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(sqrtf_dest)));
        codegen.compile_function(&sqrtf_func).unwrap();

        codegen.finalize().unwrap();

        unsafe {
            let sqrt_fn: fn(f64) -> f64 = codegen.get_typed_function("sqrt_call").unwrap();
            let sqrtf_fn: fn(f32) -> f32 = codegen.get_typed_function("sqrtf_call").unwrap();
            let sin_fn: fn(f64) -> f64 = codegen.get_typed_function("sin_call").unwrap();
            let cos_fn: fn(f64) -> f64 = codegen.get_typed_function("cos_call").unwrap();
            let exp_fn: fn(f64) -> f64 = codegen.get_typed_function("exp_call").unwrap();
            let log_fn: fn(f64) -> f64 = codegen.get_typed_function("log_call").unwrap();

            assert!((sqrt_fn(9.0) - 3.0).abs() < 1e-12);
            assert!((sqrtf_fn(9.0) - 3.0).abs() < 1e-6);
            assert!((sin_fn(0.5) - 0.5_f64.sin()).abs() < 1e-12);
            assert!((cos_fn(0.5) - 0.5_f64.cos()).abs() < 1e-12);
            assert!((exp_fn(1.0) - std::f64::consts::E).abs() < 1e-12);
            assert!((log_fn(std::f64::consts::E) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn cranelift_abs_builtin_lowers_float_and_integer_issue_7122() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut fabs_func = IrFunction::new(
            "fabs_call".to_string(),
            vec![("x".to_string(), StaticType::F64)],
            StaticType::F64,
        );
        let fabs_dest = VarRef::new("result".to_string(), StaticType::F64);
        let fabs_arg = VarRef::new("x".to_string(), StaticType::F64);
        fabs_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::Call {
                dest: Some(fabs_dest.clone()),
                func: "abs".to_string(),
                args: vec![fabs_arg],
            });
        fabs_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(fabs_dest)));
        codegen.compile_function(&fabs_func).unwrap();

        let mut iabs_func = IrFunction::new(
            "iabs_call".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let iabs_dest = VarRef::new("result".to_string(), StaticType::I64);
        let iabs_arg = VarRef::new("x".to_string(), StaticType::I64);
        iabs_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::Call {
                dest: Some(iabs_dest.clone()),
                func: "abs".to_string(),
                args: vec![iabs_arg],
            });
        iabs_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(iabs_dest)));
        codegen.compile_function(&iabs_func).unwrap();

        codegen.finalize().unwrap();

        unsafe {
            let fabs_fn: fn(f64) -> f64 = codegen.get_typed_function("fabs_call").unwrap();
            let iabs_fn: fn(i64) -> i64 = codegen.get_typed_function("iabs_call").unwrap();

            assert_eq!(fabs_fn(-3.5), 3.5);
            assert_eq!(iabs_fn(-42), 42);
            assert_eq!(iabs_fn(i64::MIN), i64::MIN);
        }
    }

    #[test]
    fn cranelift_bitwise_integer_ops_issue_7120() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let cases = [
            (
                "bitand_i64",
                BinOpKind::BitAnd,
                0b1100_i64,
                0b1010_i64,
                0b1000_i64,
            ),
            (
                "bitor_i64",
                BinOpKind::BitOr,
                0b1100_i64,
                0b1010_i64,
                0b1110_i64,
            ),
            (
                "bitxor_i64",
                BinOpKind::BitXor,
                0b1100_i64,
                0b1010_i64,
                0b0110_i64,
            ),
        ];

        for (name, op, _, _, _) in cases {
            let mut func = IrFunction::new(
                name.to_string(),
                vec![
                    ("a".to_string(), StaticType::I64),
                    ("b".to_string(), StaticType::I64),
                ],
                StaticType::I64,
            );
            let dest = VarRef::new("result".to_string(), StaticType::I64);
            let left = VarRef::new("a".to_string(), StaticType::I64);
            let right = VarRef::new("b".to_string(), StaticType::I64);
            func.entry_block_mut().unwrap().push(Instruction::BinOp {
                dest: dest.clone(),
                op,
                left,
                right,
            });
            func.entry_block_mut()
                .unwrap()
                .set_terminator(Terminator::Return(Some(dest)));
            codegen.compile_function(&func).unwrap();
        }

        codegen.finalize().unwrap();

        unsafe {
            for (name, _, left, right, expected) in cases {
                let bit_fn: fn(i64, i64) -> i64 = codegen.get_typed_function(name).unwrap();
                assert_eq!(bit_fn(left, right), expected);
            }
        }
    }

    #[test]
    fn cranelift_shift_width_and_signedness_issue_7120() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut u8_shr = IrFunction::new(
            "u8_shr".to_string(),
            vec![
                ("x".to_string(), StaticType::U8),
                ("n".to_string(), StaticType::I64),
            ],
            StaticType::U8,
        );
        let u8_dest = VarRef::new("result".to_string(), StaticType::U8);
        let u8_x = VarRef::new("x".to_string(), StaticType::U8);
        let u8_n = VarRef::new("n".to_string(), StaticType::I64);
        u8_shr.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: u8_dest.clone(),
            op: BinOpKind::Shr,
            left: u8_x,
            right: u8_n,
        });
        u8_shr
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(u8_dest)));
        codegen.compile_function(&u8_shr).unwrap();

        let mut i8_shr = IrFunction::new(
            "i8_shr".to_string(),
            vec![
                ("x".to_string(), StaticType::I8),
                ("n".to_string(), StaticType::I64),
            ],
            StaticType::I8,
        );
        let i8_dest = VarRef::new("result".to_string(), StaticType::I8);
        let i8_x = VarRef::new("x".to_string(), StaticType::I8);
        let i8_n = VarRef::new("n".to_string(), StaticType::I64);
        i8_shr.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: i8_dest.clone(),
            op: BinOpKind::Shr,
            left: i8_x,
            right: i8_n,
        });
        i8_shr
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(i8_dest)));
        codegen.compile_function(&i8_shr).unwrap();

        let mut i8_shl = IrFunction::new(
            "i8_shl".to_string(),
            vec![
                ("x".to_string(), StaticType::I8),
                ("n".to_string(), StaticType::I64),
            ],
            StaticType::I8,
        );
        let shl_dest = VarRef::new("result".to_string(), StaticType::I8);
        let shl_x = VarRef::new("x".to_string(), StaticType::I8);
        let shl_n = VarRef::new("n".to_string(), StaticType::I64);
        i8_shl.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: shl_dest.clone(),
            op: BinOpKind::Shl,
            left: shl_x,
            right: shl_n,
        });
        i8_shl
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(shl_dest)));
        codegen.compile_function(&i8_shl).unwrap();

        codegen.finalize().unwrap();

        unsafe {
            let u8_shr_fn: fn(u8, i64) -> u8 = codegen.get_typed_function("u8_shr").unwrap();
            let i8_shr_fn: fn(i8, i64) -> i8 = codegen.get_typed_function("i8_shr").unwrap();
            let i8_shl_fn: fn(i8, i64) -> i8 = codegen.get_typed_function("i8_shl").unwrap();

            assert_eq!(u8_shr_fn(0x80, 1), 0x40);
            assert_eq!(i8_shr_fn(-2, 1), -1);
            assert_eq!(i8_shl_fn(1, 2), 4);
        }
    }

    #[test]
    fn cranelift_bitnot_integer_issue_7120() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "bitnot_i64".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let operand = VarRef::new("x".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::UnaryOp {
            dest: dest.clone(),
            op: UnaryOpKind::BitNot,
            operand,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let bitnot_fn: fn(i64) -> i64 = codegen.get_typed_function("bitnot_i64").unwrap();
            assert_eq!(bitnot_fn(0b1010), !0b1010_i64);
        }
    }

    #[test]
    fn cranelift_typeassert_conversion_requires_runtime_check_issue_7111() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "float_to_int_assert".to_string(),
            vec![("x".to_string(), StaticType::F64)],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let src = VarRef::new("x".to_string(), StaticType::F64);
        func.entry_block_mut()
            .unwrap()
            .push(Instruction::TypeAssert {
                dest: dest.clone(),
                src,
                ty: StaticType::I64,
            });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        let err = codegen.compile_function(&func).unwrap_err();
        assert!(matches!(err, CraneliftError::Unsupported(_)));
        assert!(err.to_string().contains("Float64"));
        assert!(err.to_string().contains("Int64"));
        assert!(err.to_string().contains("Issue #7111"));
        assert!(err.to_string().contains("#7123"));
    }

    #[test]
    fn cranelift_unsupported_scalar_types_are_enumerated_issue_6949() {
        for ty in [StaticType::Missing] {
            let err = static_type_to_cranelift(&ty).unwrap_err();
            assert!(matches!(err, CraneliftError::TypeConversion(_)));
            assert!(err.to_string().contains("Unsupported type"));
            assert!(err.to_string().contains(&format!("{:?}", ty)));
        }
    }

    #[test]
    fn test_simple_function() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Create a simple function: fn add(a: i64, b: i64) -> i64 { a + b }
        let mut func = IrFunction::new(
            "add".to_string(),
            vec![
                ("a".to_string(), StaticType::I64),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );

        // Add instruction: result = a + b
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let left = VarRef::new("a".to_string(), StaticType::I64);
        let right = VarRef::new("b".to_string(), StaticType::I64);

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
    fn cranelift_bool_int_arithmetic_promotes_bool_operand_issue_7100() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "add_bool_int".to_string(),
            vec![
                ("a".to_string(), StaticType::Bool),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let left = VarRef::new("a".to_string(), StaticType::Bool);
        let right = VarRef::new("b".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Add,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let add_fn: fn(u8, i64) -> i64 = codegen.get_typed_function("add_bool_int").unwrap();
            assert_eq!(add_fn(1, 2), 3);
            assert_eq!(add_fn(0, 2), 2);
        }
    }

    #[test]
    fn cranelift_bool_int_comparison_promotes_bool_operand_issue_7100() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "lt_bool_int".to_string(),
            vec![
                ("a".to_string(), StaticType::Bool),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::Bool,
        );
        let dest = VarRef::new("result".to_string(), StaticType::Bool);
        let left = VarRef::new("a".to_string(), StaticType::Bool);
        let right = VarRef::new("b".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Lt,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let lt_fn: fn(u8, i64) -> u8 = codegen.get_typed_function("lt_bool_int").unwrap();
            assert_eq!(lt_fn(1, 2), 1);
            assert_eq!(lt_fn(1, 1), 0);
        }
    }

    #[test]
    fn cranelift_bool_mul_preserves_bool_result_issue_7100() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "mul_bool".to_string(),
            vec![
                ("a".to_string(), StaticType::Bool),
                ("b".to_string(), StaticType::Bool),
            ],
            StaticType::Bool,
        );
        let dest = VarRef::new("result".to_string(), StaticType::Bool);
        let left = VarRef::new("a".to_string(), StaticType::Bool);
        let right = VarRef::new("b".to_string(), StaticType::Bool);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Mul,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let mul_fn: fn(u8, u8) -> u8 = codegen.get_typed_function("mul_bool").unwrap();
            assert_eq!(mul_fn(1, 1), 1);
            assert_eq!(mul_fn(1, 0), 0);
        }
    }

    #[test]
    fn cranelift_integer_add_wraps_on_overflow_issue_7110() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "add_wrap".to_string(),
            vec![
                ("a".to_string(), StaticType::I64),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let left = VarRef::new("a".to_string(), StaticType::I64);
        let right = VarRef::new("b".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Add,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let add_fn: fn(i64, i64) -> i64 = codegen.get_typed_function("add_wrap").unwrap();
            assert_eq!(add_fn(i64::MAX, 1), i64::MIN);
        }
    }

    #[test]
    fn cranelift_integer_sub_wraps_on_overflow_issue_7110() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "sub_wrap".to_string(),
            vec![
                ("a".to_string(), StaticType::I64),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let left = VarRef::new("a".to_string(), StaticType::I64);
        let right = VarRef::new("b".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Sub,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let sub_fn: fn(i64, i64) -> i64 = codegen.get_typed_function("sub_wrap").unwrap();
            assert_eq!(sub_fn(i64::MIN, 1), i64::MAX);
        }
    }

    #[test]
    fn cranelift_integer_mul_wraps_on_overflow_issue_7110() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        let mut func = IrFunction::new(
            "mul_wrap".to_string(),
            vec![
                ("a".to_string(), StaticType::I64),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let left = VarRef::new("a".to_string(), StaticType::I64);
        let right = VarRef::new("b".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::BinOp {
            dest: dest.clone(),
            op: BinOpKind::Mul,
            left,
            right,
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(dest)));

        codegen.compile_function(&func).unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let mul_fn: fn(i64, i64) -> i64 = codegen.get_typed_function("mul_wrap").unwrap();
            assert_eq!(mul_fn(i64::MAX, 2), -2);
        }
    }

    #[test]
    fn test_function_call() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Create callee: fn double(x: i64) -> i64 { x + x }
        let mut double_fn = IrFunction::new(
            "double".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let d = VarRef::new("d".to_string(), StaticType::I64);
        let x = VarRef::new("x".to_string(), StaticType::I64);
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
            vec![("a".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let result = VarRef::new("result".to_string(), StaticType::I64);
        let a = VarRef::new("a".to_string(), StaticType::I64);
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
    fn cranelift_object_emits_dwarf_debug_sections_issue_7090() {
        use object::{Object, ObjectSection};

        let mut func = IrFunction::new(
            "debug_identity".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        func.debug_line = Some(17);
        let x = VarRef::new("x".to_string(), StaticType::I64);
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(x)));

        let mut module = IrModule::new("issue_7090".to_string());
        module.add_function(func);
        let config = CodegenConfig {
            debug_info: true,
            source_name: "issue_7090_debug.jl".to_string(),
            ..CodegenConfig::default()
        };
        let object_bytes = CraneliftObjectCodeGenerator::with_config(config)
            .unwrap()
            .generate_object(&module)
            .unwrap();

        let object_file = object::File::parse(&*object_bytes).unwrap();
        for section_name in [".debug_abbrev", ".debug_info", ".debug_line"] {
            let section = object_file
                .section_by_name(section_name)
                .unwrap_or_else(|| panic!("missing {section_name} section"));
            assert!(
                !section.data().unwrap().is_empty(),
                "{section_name} should not be empty"
            );
        }
        assert!(object_bytes
            .windows(b"issue_7090_debug.jl".len())
            .any(|window| window == b"issue_7090_debug.jl"));
        assert!(object_bytes
            .windows(b"debug_identity".len())
            .any(|window| window == b"debug_identity"));
    }

    #[test]
    fn test_pow_float() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn power(a: f64, b: f64) -> f64 { a ^ b }
        let mut func = IrFunction::new(
            "power".to_string(),
            vec![
                ("a".to_string(), StaticType::F64),
                ("b".to_string(), StaticType::F64),
            ],
            StaticType::F64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::F64);
        let left = VarRef::new("a".to_string(), StaticType::F64);
        let right = VarRef::new("b".to_string(), StaticType::F64);
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
                ("a".to_string(), StaticType::F64),
                ("b".to_string(), StaticType::F64),
            ],
            StaticType::F64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::F64);
        let left = VarRef::new("a".to_string(), StaticType::F64);
        let right = VarRef::new("b".to_string(), StaticType::F64);
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
    fn cranelift_float_comparison_nan_semantics_issue_7124() {
        for (name, op, nan_expected, normal_expected) in [
            ("nan_eq", BinOpKind::Eq, 0, 1),
            ("nan_ne", BinOpKind::Ne, 1, 0),
            ("nan_lt", BinOpKind::Lt, 0, 1),
            ("nan_le", BinOpKind::Le, 0, 1),
            ("nan_gt", BinOpKind::Gt, 0, 1),
            ("nan_ge", BinOpKind::Ge, 0, 1),
        ] {
            let mut codegen = CraneliftCodeGenerator::new().unwrap();
            let mut func = IrFunction::new(
                name.to_string(),
                vec![
                    ("a".to_string(), StaticType::F64),
                    ("b".to_string(), StaticType::F64),
                ],
                StaticType::Bool,
            );
            let dest = VarRef::new("result".to_string(), StaticType::Bool);
            let left = VarRef::new("a".to_string(), StaticType::F64);
            let right = VarRef::new("b".to_string(), StaticType::F64);
            func.entry_block_mut().unwrap().push(Instruction::BinOp {
                dest: dest.clone(),
                op,
                left,
                right,
            });
            func.entry_block_mut()
                .unwrap()
                .set_terminator(Terminator::Return(Some(dest)));

            codegen.compile_function(&func).unwrap();
            codegen.finalize().unwrap();

            unsafe {
                let cmp_fn: fn(f64, f64) -> u8 = codegen.get_typed_function(name).unwrap();
                assert_eq!(cmp_fn(f64::NAN, f64::NAN), nan_expected, "{name}(NaN, NaN)");
                assert_eq!(cmp_fn(f64::NAN, 1.0), nan_expected, "{name}(NaN, 1.0)");
                assert_eq!(cmp_fn(1.0, f64::NAN), nan_expected, "{name}(1.0, NaN)");

                let normal = match op {
                    BinOpKind::Eq => cmp_fn(1.0, 1.0),
                    BinOpKind::Ne => cmp_fn(1.0, 1.0),
                    BinOpKind::Lt | BinOpKind::Le => cmp_fn(1.0, 2.0),
                    BinOpKind::Gt | BinOpKind::Ge => cmp_fn(2.0, 1.0),
                    _ => unreachable!(),
                };
                assert_eq!(normal, normal_expected, "{name} normal comparison");
            }
        }
    }

    #[test]
    fn cranelift_signed_integer_div_rem_match_julia_issue_7119() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut div_func = IrFunction::new(
            "signed_div".to_string(),
            vec![
                ("a".to_string(), StaticType::I64),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let div_dest = VarRef::new("result".to_string(), StaticType::I64);
        div_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: div_dest.clone(),
                op: BinOpKind::Div,
                left: VarRef::new("a".to_string(), StaticType::I64),
                right: VarRef::new("b".to_string(), StaticType::I64),
            });
        div_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(div_dest)));

        let mut rem_func = IrFunction::new(
            "signed_rem".to_string(),
            vec![
                ("a".to_string(), StaticType::I64),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let rem_dest = VarRef::new("result".to_string(), StaticType::I64);
        rem_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: rem_dest.clone(),
                op: BinOpKind::Rem,
                left: VarRef::new("a".to_string(), StaticType::I64),
                right: VarRef::new("b".to_string(), StaticType::I64),
            });
        rem_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(rem_dest)));

        let mut module = IrModule::new("signed_division".to_string());
        module.add_function(div_func);
        module.add_function(rem_func);
        let result = codegen.generate_module(&module);
        assert!(
            result.is_ok(),
            "signed division module failed: {:?}",
            result.err()
        );

        unsafe {
            let signed_div: fn(i64, i64) -> i64 = codegen.get_typed_function("signed_div").unwrap();
            let signed_rem: fn(i64, i64) -> i64 = codegen.get_typed_function("signed_rem").unwrap();

            assert_eq!(signed_div(-5, 3), -1);
            assert_eq!(signed_div(5, -3), -1);
            assert_eq!(signed_div(-5, -3), 1);
            assert_eq!(signed_rem(-5, 3), -2);
            assert_eq!(signed_rem(5, -3), 2);
            assert_eq!(signed_rem(-5, -3), -2);
        }
    }

    #[test]
    fn cranelift_unsigned_integer_div_rem_use_unsigned_ops_issue_7119() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut div_func = IrFunction::new(
            "unsigned_div".to_string(),
            vec![
                ("a".to_string(), StaticType::U64),
                ("b".to_string(), StaticType::U64),
            ],
            StaticType::U64,
        );
        let div_dest = VarRef::new("result".to_string(), StaticType::U64);
        div_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: div_dest.clone(),
                op: BinOpKind::Div,
                left: VarRef::new("a".to_string(), StaticType::U64),
                right: VarRef::new("b".to_string(), StaticType::U64),
            });
        div_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(div_dest)));

        let mut rem_func = IrFunction::new(
            "unsigned_rem".to_string(),
            vec![
                ("a".to_string(), StaticType::U64),
                ("b".to_string(), StaticType::U64),
            ],
            StaticType::U64,
        );
        let rem_dest = VarRef::new("result".to_string(), StaticType::U64);
        rem_func
            .entry_block_mut()
            .unwrap()
            .push(Instruction::BinOp {
                dest: rem_dest.clone(),
                op: BinOpKind::Rem,
                left: VarRef::new("a".to_string(), StaticType::U64),
                right: VarRef::new("b".to_string(), StaticType::U64),
            });
        rem_func
            .entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(rem_dest)));

        let mut module = IrModule::new("unsigned_division".to_string());
        module.add_function(div_func);
        module.add_function(rem_func);
        let result = codegen.generate_module(&module);
        assert!(
            result.is_ok(),
            "unsigned division module failed: {:?}",
            result.err()
        );

        unsafe {
            let unsigned_div: fn(u64, u64) -> u64 =
                codegen.get_typed_function("unsigned_div").unwrap();
            let unsigned_rem: fn(u64, u64) -> u64 =
                codegen.get_typed_function("unsigned_rem").unwrap();

            assert_eq!(unsigned_div(u64::MAX, 2), u64::MAX / 2);
            assert_eq!(unsigned_rem(u64::MAX, 2), u64::MAX % 2);
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
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );

        let x = VarRef::new("x".to_string(), StaticType::I64);
        let cond = VarRef::new("cond".to_string(), StaticType::Bool);
        let neg_x = VarRef::new("neg_x".to_string(), StaticType::I64);
        let result = VarRef::new("result".to_string(), StaticType::I64);

        // Entry block: cond = x < 0; branch cond, neg_block, pos_block
        let zero_const = VarRef::new("zero".to_string(), StaticType::I64);
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
    fn cranelift_short_circuit_branch_phi_truth_table_issue_7115() {
        use crate::aot::ir::BasicBlock;

        fn bool_var(name: &str) -> VarRef {
            VarRef::new(name.to_string(), StaticType::Bool)
        }

        fn build_short_circuit_func(name: &str, is_and: bool) -> IrFunction {
            let mut func = IrFunction::new(
                name.to_string(),
                vec![
                    ("left".to_string(), StaticType::Bool),
                    ("right".to_string(), StaticType::Bool),
                ],
                StaticType::Bool,
            );
            let left = bool_var("left");
            let right = bool_var("right");
            let const_result = bool_var("const_result");
            let result = bool_var("result");

            let entry = func.entry_block_mut().unwrap();
            let (then_block, else_block, const_value) = if is_and {
                ("rhs", "const", false)
            } else {
                ("const", "rhs", true)
            };
            entry.set_terminator(Terminator::Branch {
                cond: left,
                then_block: then_block.to_string(),
                else_block: else_block.to_string(),
            });

            let mut rhs = BasicBlock::new("rhs".to_string());
            rhs.set_terminator(Terminator::Jump("join".to_string()));
            func.add_block(rhs);

            let mut const_block = BasicBlock::new("const".to_string());
            const_block.push(Instruction::LoadConst {
                dest: const_result.clone(),
                value: ConstValue::Bool(const_value),
            });
            const_block.set_terminator(Terminator::Jump("join".to_string()));
            func.add_block(const_block);

            let mut join = BasicBlock::new("join".to_string());
            join.push(Instruction::Phi {
                dest: result.clone(),
                incoming: vec![
                    ("rhs".to_string(), right),
                    ("const".to_string(), const_result),
                ],
            });
            join.set_terminator(Terminator::Return(Some(result)));
            func.add_block(join);

            func
        }

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        codegen
            .compile_function(&build_short_circuit_func("short_and", true))
            .unwrap();
        codegen
            .compile_function(&build_short_circuit_func("short_or", false))
            .unwrap();
        codegen.finalize().unwrap();

        unsafe {
            let short_and: fn(u8, u8) -> u8 = codegen.get_typed_function("short_and").unwrap();
            let short_or: fn(u8, u8) -> u8 = codegen.get_typed_function("short_or").unwrap();

            assert_eq!(short_and(0, 0), 0);
            assert_eq!(short_and(0, 1), 0);
            assert_eq!(short_and(1, 0), 0);
            assert_eq!(short_and(1, 1), 1);

            assert_eq!(short_or(0, 0), 0);
            assert_eq!(short_or(0, 1), 1);
            assert_eq!(short_or(1, 0), 1);
            assert_eq!(short_or(1, 1), 1);
        }
    }

    #[test]
    fn cranelift_entry_phi_without_block_param_is_rejected_issue_7113() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut func = IrFunction::new("bad_entry_phi".to_string(), vec![], StaticType::I64);
        let result = VarRef::new("result".to_string(), StaticType::I64);
        func.entry_block_mut().unwrap().push(Instruction::Phi {
            dest: result.clone(),
            incoming: vec![],
        });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Return(Some(result)));

        let err = codegen.compile_function(&func).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing its block parameter mapping"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn cranelift_missing_phi_incoming_is_rejected_issue_7113() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut func = IrFunction::new("bad_phi_edge".to_string(), vec![], StaticType::I64);
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        let result = VarRef::new("result".to_string(), StaticType::I64);

        func.entry_block_mut()
            .unwrap()
            .push(Instruction::LoadConst {
                dest: zero.clone(),
                value: ConstValue::Int64(0),
            });
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Jump("merge".to_string()));

        let mut merge = BasicBlock::new("merge".to_string());
        merge.push(Instruction::Phi {
            dest: result.clone(),
            incoming: vec![("other".to_string(), zero)],
        });
        merge.set_terminator(Terminator::Return(Some(result)));
        func.add_block(merge);

        let err = codegen.compile_function(&func).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing phi incoming values for edge entry -> merge"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn cranelift_loop_backedge_phi_sums_i64_issue_7112() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn sum_to_n(n: i64) -> i64 {
        //   i = 0
        //   acc = 0
        //   while i <= n
        //     acc += i
        //     i += 1
        //   end
        //   acc
        // }
        let mut func = IrFunction::new(
            "sum_to_n".to_string(),
            vec![("n".to_string(), StaticType::I64)],
            StaticType::I64,
        );

        let n = VarRef::new("n".to_string(), StaticType::I64);
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        let one = VarRef::new("one".to_string(), StaticType::I64);
        let i = VarRef::new("i".to_string(), StaticType::I64);
        let acc = VarRef::new("acc".to_string(), StaticType::I64);
        let cond = VarRef::new("cond".to_string(), StaticType::Bool);
        let i_next = VarRef::new("i_next".to_string(), StaticType::I64);
        let acc_next = VarRef::new("acc_next".to_string(), StaticType::I64);

        let entry = func.entry_block_mut().unwrap();
        entry.push(Instruction::LoadConst {
            dest: zero.clone(),
            value: ConstValue::Int64(0),
        });
        entry.push(Instruction::LoadConst {
            dest: one.clone(),
            value: ConstValue::Int64(1),
        });
        entry.set_terminator(Terminator::Jump("header".to_string()));

        let mut header = BasicBlock::new("header".to_string());
        header.push(Instruction::Phi {
            dest: i.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("body".to_string(), i_next.clone()),
            ],
        });
        header.push(Instruction::Phi {
            dest: acc.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("body".to_string(), acc_next.clone()),
            ],
        });
        header.push(Instruction::BinOp {
            dest: cond.clone(),
            op: BinOpKind::Le,
            left: i.clone(),
            right: n.clone(),
        });
        header.set_terminator(Terminator::Branch {
            cond,
            then_block: "body".to_string(),
            else_block: "exit".to_string(),
        });
        func.add_block(header);

        let mut body = BasicBlock::new("body".to_string());
        body.push(Instruction::BinOp {
            dest: acc_next.clone(),
            op: BinOpKind::Add,
            left: acc.clone(),
            right: i.clone(),
        });
        body.push(Instruction::BinOp {
            dest: i_next.clone(),
            op: BinOpKind::Add,
            left: i.clone(),
            right: one,
        });
        body.set_terminator(Terminator::Jump("header".to_string()));
        func.add_block(body);

        let mut exit = BasicBlock::new("exit".to_string());
        exit.set_terminator(Terminator::Return(Some(acc)));
        func.add_block(exit);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "loop back-edge phi compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let sum_to_n: fn(i64) -> i64 = codegen.get_typed_function("sum_to_n").unwrap();
            assert_eq!(sum_to_n(0), 0);
            assert_eq!(sum_to_n(5), 15);
            assert_eq!(sum_to_n(10), 55);
        }
    }

    #[test]
    fn cranelift_multiple_backedges_phi_issue_7112() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Keep the loop header reachable from two distinct latch blocks.
        let mut func = IrFunction::new(
            "count_by_two_paths".to_string(),
            vec![("n".to_string(), StaticType::I64)],
            StaticType::I64,
        );

        let n = VarRef::new("n".to_string(), StaticType::I64);
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        let one = VarRef::new("one".to_string(), StaticType::I64);
        let count = VarRef::new("count".to_string(), StaticType::I64);
        let cond = VarRef::new("cond".to_string(), StaticType::Bool);
        let count_then = VarRef::new("count_then".to_string(), StaticType::I64);
        let count_else = VarRef::new("count_else".to_string(), StaticType::I64);
        let parity = VarRef::new("parity".to_string(), StaticType::Bool);

        let entry = func.entry_block_mut().unwrap();
        entry.push(Instruction::LoadConst {
            dest: zero.clone(),
            value: ConstValue::Int64(0),
        });
        entry.push(Instruction::LoadConst {
            dest: one.clone(),
            value: ConstValue::Int64(1),
        });
        entry.set_terminator(Terminator::Jump("header".to_string()));

        let mut header = BasicBlock::new("header".to_string());
        header.push(Instruction::Phi {
            dest: count.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("then_latch".to_string(), count_then.clone()),
                ("else_latch".to_string(), count_else.clone()),
            ],
        });
        header.push(Instruction::BinOp {
            dest: cond.clone(),
            op: BinOpKind::Lt,
            left: count.clone(),
            right: n.clone(),
        });
        header.set_terminator(Terminator::Branch {
            cond,
            then_block: "dispatch".to_string(),
            else_block: "exit".to_string(),
        });
        func.add_block(header);

        let mut dispatch = BasicBlock::new("dispatch".to_string());
        dispatch.push(Instruction::BinOp {
            dest: parity.clone(),
            op: BinOpKind::Eq,
            left: count.clone(),
            right: zero.clone(),
        });
        dispatch.set_terminator(Terminator::Branch {
            cond: parity,
            then_block: "then_latch".to_string(),
            else_block: "else_latch".to_string(),
        });
        func.add_block(dispatch);

        let mut then_latch = BasicBlock::new("then_latch".to_string());
        then_latch.push(Instruction::BinOp {
            dest: count_then.clone(),
            op: BinOpKind::Add,
            left: count.clone(),
            right: one.clone(),
        });
        then_latch.set_terminator(Terminator::Jump("header".to_string()));
        func.add_block(then_latch);

        let mut else_latch = BasicBlock::new("else_latch".to_string());
        else_latch.push(Instruction::BinOp {
            dest: count_else.clone(),
            op: BinOpKind::Add,
            left: count.clone(),
            right: one,
        });
        else_latch.set_terminator(Terminator::Jump("header".to_string()));
        func.add_block(else_latch);

        let mut exit = BasicBlock::new("exit".to_string());
        exit.set_terminator(Terminator::Return(Some(count)));
        func.add_block(exit);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "multi-back-edge phi compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let count_by_two_paths: fn(i64) -> i64 =
                codegen.get_typed_function("count_by_two_paths").unwrap();
            assert_eq!(count_by_two_paths(0), 0);
            assert_eq!(count_by_two_paths(1), 1);
            assert_eq!(count_by_two_paths(3), 3);
        }
    }

    #[test]
    fn cranelift_nested_loop_backedge_phi_issue_7112() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Count the iterations of a nested loop:
        //   total = 0
        //   for _ in 1:n
        //     for _ in 1:m
        //       total += 1
        //     end
        //   end
        let mut func = IrFunction::new(
            "nested_count".to_string(),
            vec![
                ("n".to_string(), StaticType::I64),
                ("m".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );

        let n = VarRef::new("n".to_string(), StaticType::I64);
        let m = VarRef::new("m".to_string(), StaticType::I64);
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        let one = VarRef::new("one".to_string(), StaticType::I64);
        let i = VarRef::new("i".to_string(), StaticType::I64);
        let total = VarRef::new("total".to_string(), StaticType::I64);
        let outer_cond = VarRef::new("outer_cond".to_string(), StaticType::Bool);
        let j = VarRef::new("j".to_string(), StaticType::I64);
        let inner_total = VarRef::new("inner_total".to_string(), StaticType::I64);
        let inner_cond = VarRef::new("inner_cond".to_string(), StaticType::Bool);
        let j_next = VarRef::new("j_next".to_string(), StaticType::I64);
        let inner_total_next = VarRef::new("inner_total_next".to_string(), StaticType::I64);
        let i_next = VarRef::new("i_next".to_string(), StaticType::I64);
        let total_after_inner = VarRef::new("total_after_inner".to_string(), StaticType::I64);

        let entry = func.entry_block_mut().unwrap();
        entry.push(Instruction::LoadConst {
            dest: zero.clone(),
            value: ConstValue::Int64(0),
        });
        entry.push(Instruction::LoadConst {
            dest: one.clone(),
            value: ConstValue::Int64(1),
        });
        entry.set_terminator(Terminator::Jump("outer_header".to_string()));

        let mut outer_header = BasicBlock::new("outer_header".to_string());
        outer_header.push(Instruction::Phi {
            dest: i.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("outer_latch".to_string(), i_next.clone()),
            ],
        });
        outer_header.push(Instruction::Phi {
            dest: total.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("outer_latch".to_string(), total_after_inner.clone()),
            ],
        });
        outer_header.push(Instruction::BinOp {
            dest: outer_cond.clone(),
            op: BinOpKind::Lt,
            left: i.clone(),
            right: n,
        });
        outer_header.set_terminator(Terminator::Branch {
            cond: outer_cond,
            then_block: "pre_inner".to_string(),
            else_block: "exit".to_string(),
        });
        func.add_block(outer_header);

        let mut pre_inner = BasicBlock::new("pre_inner".to_string());
        pre_inner.set_terminator(Terminator::Jump("inner_header".to_string()));
        func.add_block(pre_inner);

        let mut inner_header = BasicBlock::new("inner_header".to_string());
        inner_header.push(Instruction::Phi {
            dest: j.clone(),
            incoming: vec![
                ("pre_inner".to_string(), zero.clone()),
                ("inner_body".to_string(), j_next.clone()),
            ],
        });
        inner_header.push(Instruction::Phi {
            dest: inner_total.clone(),
            incoming: vec![
                ("pre_inner".to_string(), total.clone()),
                ("inner_body".to_string(), inner_total_next.clone()),
            ],
        });
        inner_header.push(Instruction::BinOp {
            dest: inner_cond.clone(),
            op: BinOpKind::Lt,
            left: j.clone(),
            right: m,
        });
        inner_header.set_terminator(Terminator::Branch {
            cond: inner_cond,
            then_block: "inner_body".to_string(),
            else_block: "outer_latch".to_string(),
        });
        func.add_block(inner_header);

        let mut inner_body = BasicBlock::new("inner_body".to_string());
        inner_body.push(Instruction::BinOp {
            dest: inner_total_next.clone(),
            op: BinOpKind::Add,
            left: inner_total.clone(),
            right: one.clone(),
        });
        inner_body.push(Instruction::BinOp {
            dest: j_next.clone(),
            op: BinOpKind::Add,
            left: j,
            right: one.clone(),
        });
        inner_body.set_terminator(Terminator::Jump("inner_header".to_string()));
        func.add_block(inner_body);

        let mut outer_latch = BasicBlock::new("outer_latch".to_string());
        outer_latch.push(Instruction::BinOp {
            dest: i_next.clone(),
            op: BinOpKind::Add,
            left: i,
            right: one,
        });
        outer_latch.push(Instruction::Copy {
            dest: total_after_inner.clone(),
            src: inner_total,
        });
        outer_latch.set_terminator(Terminator::Jump("outer_header".to_string()));
        func.add_block(outer_latch);

        let mut exit = BasicBlock::new("exit".to_string());
        exit.set_terminator(Terminator::Return(Some(total)));
        func.add_block(exit);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "nested loop phi compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let nested_count: fn(i64, i64) -> i64 =
                codegen.get_typed_function("nested_count").unwrap();
            assert_eq!(nested_count(0, 5), 0);
            assert_eq!(nested_count(2, 0), 0);
            assert_eq!(nested_count(3, 4), 12);
        }
    }

    #[test]
    fn cranelift_continue_targets_loop_latch_issue_7116() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Sum 0:n-1 while skipping `skip`. The continue edge must jump to the
        // latch that increments i, not back to the header with stale state.
        let mut func = IrFunction::new(
            "sum_skip".to_string(),
            vec![
                ("n".to_string(), StaticType::I64),
                ("skip".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );

        let n = VarRef::new("n".to_string(), StaticType::I64);
        let skip = VarRef::new("skip".to_string(), StaticType::I64);
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        let one = VarRef::new("one".to_string(), StaticType::I64);
        let i = VarRef::new("i".to_string(), StaticType::I64);
        let acc = VarRef::new("acc".to_string(), StaticType::I64);
        let loop_cond = VarRef::new("loop_cond".to_string(), StaticType::Bool);
        let skip_cond = VarRef::new("skip_cond".to_string(), StaticType::Bool);
        let body_acc = VarRef::new("body_acc".to_string(), StaticType::I64);
        let latch_acc = VarRef::new("latch_acc".to_string(), StaticType::I64);
        let i_next = VarRef::new("i_next".to_string(), StaticType::I64);

        let entry = func.entry_block_mut().unwrap();
        entry.push(Instruction::LoadConst {
            dest: zero.clone(),
            value: ConstValue::Int64(0),
        });
        entry.push(Instruction::LoadConst {
            dest: one.clone(),
            value: ConstValue::Int64(1),
        });
        entry.set_terminator(Terminator::Jump("header".to_string()));

        let mut header = BasicBlock::new("header".to_string());
        header.push(Instruction::Phi {
            dest: i.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("latch".to_string(), i_next.clone()),
            ],
        });
        header.push(Instruction::Phi {
            dest: acc.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("latch".to_string(), latch_acc.clone()),
            ],
        });
        header.push(Instruction::BinOp {
            dest: loop_cond.clone(),
            op: BinOpKind::Lt,
            left: i.clone(),
            right: n,
        });
        header.set_terminator(Terminator::Branch {
            cond: loop_cond,
            then_block: "check_continue".to_string(),
            else_block: "exit".to_string(),
        });
        func.add_block(header);

        let mut check_continue = BasicBlock::new("check_continue".to_string());
        check_continue.push(Instruction::BinOp {
            dest: skip_cond.clone(),
            op: BinOpKind::Eq,
            left: i.clone(),
            right: skip,
        });
        check_continue.set_terminator(Terminator::Branch {
            cond: skip_cond,
            then_block: "latch".to_string(),
            else_block: "body".to_string(),
        });
        func.add_block(check_continue);

        let mut body = BasicBlock::new("body".to_string());
        body.push(Instruction::BinOp {
            dest: body_acc.clone(),
            op: BinOpKind::Add,
            left: acc.clone(),
            right: i.clone(),
        });
        body.set_terminator(Terminator::Jump("latch".to_string()));
        func.add_block(body);

        let mut latch = BasicBlock::new("latch".to_string());
        latch.push(Instruction::Phi {
            dest: latch_acc.clone(),
            incoming: vec![
                ("check_continue".to_string(), acc.clone()),
                ("body".to_string(), body_acc),
            ],
        });
        latch.push(Instruction::BinOp {
            dest: i_next.clone(),
            op: BinOpKind::Add,
            left: i,
            right: one,
        });
        latch.set_terminator(Terminator::Jump("header".to_string()));
        func.add_block(latch);

        let mut exit = BasicBlock::new("exit".to_string());
        exit.set_terminator(Terminator::Return(Some(acc)));
        func.add_block(exit);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "continue target compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let sum_skip: fn(i64, i64) -> i64 = codegen.get_typed_function("sum_skip").unwrap();
            assert_eq!(sum_skip(5, 2), 8);
            assert_eq!(sum_skip(5, 99), 10);
        }
    }

    #[test]
    fn cranelift_nested_break_targets_inner_exit_issue_7116() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // Count inner loop iterations for each outer iteration, breaking only
        // the inner loop when j == break_at. If the break target escapes the
        // outer loop, nested_break_count(3, 5, 2) would return 2 instead of 6.
        let mut func = IrFunction::new(
            "nested_break_count".to_string(),
            vec![
                ("n".to_string(), StaticType::I64),
                ("m".to_string(), StaticType::I64),
                ("break_at".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );

        let n = VarRef::new("n".to_string(), StaticType::I64);
        let m = VarRef::new("m".to_string(), StaticType::I64);
        let break_at = VarRef::new("break_at".to_string(), StaticType::I64);
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        let one = VarRef::new("one".to_string(), StaticType::I64);
        let i = VarRef::new("i".to_string(), StaticType::I64);
        let total = VarRef::new("total".to_string(), StaticType::I64);
        let outer_cond = VarRef::new("outer_cond".to_string(), StaticType::Bool);
        let j = VarRef::new("j".to_string(), StaticType::I64);
        let inner_total = VarRef::new("inner_total".to_string(), StaticType::I64);
        let inner_cond = VarRef::new("inner_cond".to_string(), StaticType::Bool);
        let break_cond = VarRef::new("break_cond".to_string(), StaticType::Bool);
        let inner_total_next = VarRef::new("inner_total_next".to_string(), StaticType::I64);
        let j_next = VarRef::new("j_next".to_string(), StaticType::I64);
        let total_after_inner = VarRef::new("total_after_inner".to_string(), StaticType::I64);
        let i_next = VarRef::new("i_next".to_string(), StaticType::I64);

        let entry = func.entry_block_mut().unwrap();
        entry.push(Instruction::LoadConst {
            dest: zero.clone(),
            value: ConstValue::Int64(0),
        });
        entry.push(Instruction::LoadConst {
            dest: one.clone(),
            value: ConstValue::Int64(1),
        });
        entry.set_terminator(Terminator::Jump("outer_header".to_string()));

        let mut outer_header = BasicBlock::new("outer_header".to_string());
        outer_header.push(Instruction::Phi {
            dest: i.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("outer_latch".to_string(), i_next.clone()),
            ],
        });
        outer_header.push(Instruction::Phi {
            dest: total.clone(),
            incoming: vec![
                ("entry".to_string(), zero.clone()),
                ("outer_latch".to_string(), total_after_inner.clone()),
            ],
        });
        outer_header.push(Instruction::BinOp {
            dest: outer_cond.clone(),
            op: BinOpKind::Lt,
            left: i.clone(),
            right: n,
        });
        outer_header.set_terminator(Terminator::Branch {
            cond: outer_cond,
            then_block: "inner_header".to_string(),
            else_block: "exit".to_string(),
        });
        func.add_block(outer_header);

        let mut inner_header = BasicBlock::new("inner_header".to_string());
        inner_header.push(Instruction::Phi {
            dest: j.clone(),
            incoming: vec![
                ("outer_header".to_string(), zero.clone()),
                ("inner_body".to_string(), j_next.clone()),
            ],
        });
        inner_header.push(Instruction::Phi {
            dest: inner_total.clone(),
            incoming: vec![
                ("outer_header".to_string(), total.clone()),
                ("inner_body".to_string(), inner_total_next.clone()),
            ],
        });
        inner_header.push(Instruction::BinOp {
            dest: inner_cond.clone(),
            op: BinOpKind::Lt,
            left: j.clone(),
            right: m,
        });
        inner_header.set_terminator(Terminator::Branch {
            cond: inner_cond,
            then_block: "break_check".to_string(),
            else_block: "outer_latch".to_string(),
        });
        func.add_block(inner_header);

        let mut break_check = BasicBlock::new("break_check".to_string());
        break_check.push(Instruction::BinOp {
            dest: break_cond.clone(),
            op: BinOpKind::Eq,
            left: j.clone(),
            right: break_at,
        });
        break_check.set_terminator(Terminator::Branch {
            cond: break_cond,
            then_block: "outer_latch".to_string(),
            else_block: "inner_body".to_string(),
        });
        func.add_block(break_check);

        let mut inner_body = BasicBlock::new("inner_body".to_string());
        inner_body.push(Instruction::BinOp {
            dest: inner_total_next.clone(),
            op: BinOpKind::Add,
            left: inner_total.clone(),
            right: one.clone(),
        });
        inner_body.push(Instruction::BinOp {
            dest: j_next.clone(),
            op: BinOpKind::Add,
            left: j,
            right: one.clone(),
        });
        inner_body.set_terminator(Terminator::Jump("inner_header".to_string()));
        func.add_block(inner_body);

        let mut outer_latch = BasicBlock::new("outer_latch".to_string());
        outer_latch.push(Instruction::Phi {
            dest: total_after_inner.clone(),
            incoming: vec![
                ("inner_header".to_string(), inner_total.clone()),
                ("break_check".to_string(), inner_total),
            ],
        });
        outer_latch.push(Instruction::BinOp {
            dest: i_next.clone(),
            op: BinOpKind::Add,
            left: i,
            right: one,
        });
        outer_latch.set_terminator(Terminator::Jump("outer_header".to_string()));
        func.add_block(outer_latch);

        let mut exit = BasicBlock::new("exit".to_string());
        exit.set_terminator(Terminator::Return(Some(total)));
        func.add_block(exit);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "nested break target compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let nested_break_count: fn(i64, i64, i64) -> i64 =
                codegen.get_typed_function("nested_break_count").unwrap();
            assert_eq!(nested_break_count(3, 5, 2), 6);
            assert_eq!(nested_break_count(2, 4, 99), 8);
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
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );

        let x = VarRef::new("x".to_string(), StaticType::I64);

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
        let c10 = VarRef::new("c10".to_string(), StaticType::I64);
        case1.push(Instruction::LoadConst {
            dest: c10.clone(),
            value: ConstValue::Int64(10),
        });
        case1.set_terminator(Terminator::Return(Some(c10)));
        func.add_block(case1);

        // case2: return 20
        let mut case2 = BasicBlock::new("case2".to_string());
        let c20 = VarRef::new("c20".to_string(), StaticType::I64);
        case2.push(Instruction::LoadConst {
            dest: c20.clone(),
            value: ConstValue::Int64(20),
        });
        case2.set_terminator(Terminator::Return(Some(c20)));
        func.add_block(case2);

        // default: return 0
        let mut default_blk = BasicBlock::new("default".to_string());
        let c0 = VarRef::new("c0".to_string(), StaticType::I64);
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
    fn cranelift_switch_empty_cases_jump_default_issue_7114() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut func = IrFunction::new(
            "switch_empty".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let x = VarRef::new("x".to_string(), StaticType::I64);
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Switch {
                value: x,
                cases: vec![],
                default: "default".to_string(),
            });

        let mut default_blk = BasicBlock::new("default".to_string());
        let result = VarRef::new("result".to_string(), StaticType::I64);
        default_blk.push(Instruction::LoadConst {
            dest: result.clone(),
            value: ConstValue::Int64(42),
        });
        default_blk.set_terminator(Terminator::Return(Some(result)));
        func.add_block(default_blk);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "empty switch compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let switch_empty: fn(i64) -> i64 = codegen.get_typed_function("switch_empty").unwrap();
            assert_eq!(switch_empty(0), 42);
            assert_eq!(switch_empty(99), 42);
        }
    }

    #[test]
    fn cranelift_switch_bool_key_issue_7114() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut func = IrFunction::new(
            "switch_bool".to_string(),
            vec![("flag".to_string(), StaticType::Bool)],
            StaticType::I64,
        );
        let flag = VarRef::new("flag".to_string(), StaticType::Bool);
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Switch {
                value: flag,
                cases: vec![(ConstValue::Bool(true), "case_true".to_string())],
                default: "case_false".to_string(),
            });

        let mut case_true = BasicBlock::new("case_true".to_string());
        let one = VarRef::new("one".to_string(), StaticType::I64);
        case_true.push(Instruction::LoadConst {
            dest: one.clone(),
            value: ConstValue::Int64(1),
        });
        case_true.set_terminator(Terminator::Return(Some(one)));
        func.add_block(case_true);

        let mut case_false = BasicBlock::new("case_false".to_string());
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        case_false.push(Instruction::LoadConst {
            dest: zero.clone(),
            value: ConstValue::Int64(0),
        });
        case_false.set_terminator(Terminator::Return(Some(zero)));
        func.add_block(case_false);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "Bool switch compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let switch_bool: fn(bool) -> i64 = codegen.get_typed_function("switch_bool").unwrap();
            assert_eq!(switch_bool(true), 1);
            assert_eq!(switch_bool(false), 0);
        }
    }

    #[test]
    fn cranelift_switch_targets_phi_merge_issue_7114() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut func = IrFunction::new(
            "switch_phi".to_string(),
            vec![("x".to_string(), StaticType::I64)],
            StaticType::I64,
        );
        let x = VarRef::new("x".to_string(), StaticType::I64);
        let case1_value = VarRef::new("case1_value".to_string(), StaticType::I64);
        let case2_value = VarRef::new("case2_value".to_string(), StaticType::I64);
        let default_value = VarRef::new("default_value".to_string(), StaticType::I64);
        let result = VarRef::new("result".to_string(), StaticType::I64);

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

        let mut case1 = BasicBlock::new("case1".to_string());
        case1.push(Instruction::LoadConst {
            dest: case1_value.clone(),
            value: ConstValue::Int64(10),
        });
        case1.set_terminator(Terminator::Jump("merge".to_string()));
        func.add_block(case1);

        let mut case2 = BasicBlock::new("case2".to_string());
        case2.push(Instruction::LoadConst {
            dest: case2_value.clone(),
            value: ConstValue::Int64(20),
        });
        case2.set_terminator(Terminator::Jump("merge".to_string()));
        func.add_block(case2);

        let mut default_blk = BasicBlock::new("default".to_string());
        default_blk.push(Instruction::LoadConst {
            dest: default_value.clone(),
            value: ConstValue::Int64(30),
        });
        default_blk.set_terminator(Terminator::Jump("merge".to_string()));
        func.add_block(default_blk);

        let mut merge = BasicBlock::new("merge".to_string());
        merge.push(Instruction::Phi {
            dest: result.clone(),
            incoming: vec![
                ("case1".to_string(), case1_value),
                ("case2".to_string(), case2_value),
                ("default".to_string(), default_value),
            ],
        });
        merge.set_terminator(Terminator::Return(Some(result)));
        func.add_block(merge);

        let compile_result = codegen.compile_function(&func);
        assert!(
            compile_result.is_ok(),
            "switch-to-phi compilation failed: {:?}",
            compile_result.err()
        );
        codegen.finalize().unwrap();

        unsafe {
            let switch_phi: fn(i64) -> i64 = codegen.get_typed_function("switch_phi").unwrap();
            assert_eq!(switch_phi(1), 10);
            assert_eq!(switch_phi(2), 20);
            assert_eq!(switch_phi(3), 30);
        }
    }

    #[test]
    fn cranelift_switch_float_key_is_gated_issue_7114() {
        use crate::aot::ir::BasicBlock;

        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        let mut func = IrFunction::new(
            "switch_float".to_string(),
            vec![("x".to_string(), StaticType::F64)],
            StaticType::I64,
        );
        let x = VarRef::new("x".to_string(), StaticType::F64);
        func.entry_block_mut()
            .unwrap()
            .set_terminator(Terminator::Switch {
                value: x,
                cases: vec![(ConstValue::Float64(1.0), "case1".to_string())],
                default: "default".to_string(),
            });

        let mut case1 = BasicBlock::new("case1".to_string());
        let one = VarRef::new("one".to_string(), StaticType::I64);
        case1.push(Instruction::LoadConst {
            dest: one.clone(),
            value: ConstValue::Int64(1),
        });
        case1.set_terminator(Terminator::Return(Some(one)));
        func.add_block(case1);

        let mut default_blk = BasicBlock::new("default".to_string());
        let zero = VarRef::new("zero".to_string(), StaticType::I64);
        default_blk.push(Instruction::LoadConst {
            dest: zero.clone(),
            value: ConstValue::Int64(0),
        });
        default_blk.set_terminator(Terminator::Return(Some(zero)));
        func.add_block(default_blk);

        let err = codegen.compile_function(&func).unwrap_err();
        assert!(
            err.to_string()
                .contains("does not yet lower switch on `Float64` values (Issue #7114)"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_integer_pow() {
        let mut codegen = CraneliftCodeGenerator::new().unwrap();

        // fn int_pow(a: i64, b: i64) -> i64 { a ^ b }
        let mut func = IrFunction::new(
            "int_pow".to_string(),
            vec![
                ("a".to_string(), StaticType::I64),
                ("b".to_string(), StaticType::I64),
            ],
            StaticType::I64,
        );
        let dest = VarRef::new("result".to_string(), StaticType::I64);
        let left = VarRef::new("a".to_string(), StaticType::I64);
        let right = VarRef::new("b".to_string(), StaticType::I64);
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
