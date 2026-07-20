//! Type stability analyzer.
//!
//! This module implements the core analysis logic for checking type stability.

use crate::bytecode::{ArrayElementType, ValueType};
use crate::compile::abstract_interp::{usage_analysis, InferenceEngine, StructTypeInfo};
use crate::compile::collect_block_functions;
use crate::compile::context::StructInfo;
use crate::compile::context::StructRegistry;
use crate::compile::inference::{
    collect_global_types_for_inference, widen_non_const_globals_for_binding_inference,
};
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::method_table::MethodSig;
use crate::compile::tfuncs::TransferFunctions;
use crate::compile::type_stability::analysis_report::{
    InferenceProvenance, TypeStabilityAnalysisReport,
};
use crate::compile::type_stability::reason::TypeStabilityReason;
use crate::compile::type_stability::report::FunctionStabilityReport;
use crate::ir::core::{Expr, Function, Program, Stmt};
use crate::runtime_types::bridge::{
    lattice_to_parametric_julia_type, lattice_to_value_type,
    value_type_to_lattice_with_struct_table,
};
use crate::types::JuliaType;
use std::collections::HashMap;

/// Configuration for the type stability analyzer.
#[derive(Clone, Debug)]
pub struct AnalysisConfig {
    /// Whether to include base library functions in the analysis.
    pub include_base_functions: bool,

    /// Whether to analyze only user-defined functions.
    pub user_functions_only: bool,

    /// Whether to treat untyped parameters as type-unstable.
    pub strict_parameter_typing: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            include_base_functions: false,
            user_functions_only: true,
            strict_parameter_typing: false,
        }
    }
}

/// Type stability analyzer.
///
/// Analyzes functions in a program to determine their type stability.
/// A function is type-stable if its return type can be uniquely determined
/// from its input types (i.e., returns Concrete or Const, not Top or Union).
pub struct TypeStabilityAnalyzer {
    /// Configuration for the analysis.
    config: AnalysisConfig,

    /// The inference engine for type inference.
    engine: InferenceEngine,

    /// Transfer functions for usage-based parameter inference.
    tfuncs: TransferFunctions,
}

/// Return-type facts collected from the same shared inference engine shape used
/// by production code generation.
#[derive(Clone, Debug, Default)]
pub struct ProductionInferenceFacts {
    return_types_by_function_index: HashMap<usize, LatticeType>,
}

impl ProductionInferenceFacts {
    fn insert_return_type(&mut self, function_index: usize, return_type: LatticeType) {
        self.return_types_by_function_index
            .insert(function_index, return_type);
    }

    fn return_type_for(&self, function_index: usize) -> Option<&LatticeType> {
        self.return_types_by_function_index.get(&function_index)
    }
}

fn build_type_stability_struct_table(program: &Program) -> StructRegistry {
    let mut struct_table = StructRegistry::new();
    for (type_id, struct_def) in program.structs.iter().enumerate() {
        struct_table.insert(
            struct_def.name.clone(),
            StructInfo {
                type_id,
                is_mutable: struct_def.is_mutable,
                fields: Vec::new(),
                has_inner_constructor: !struct_def.inner_constructors.is_empty(),
            },
        );
    }

    let name_to_type_id: HashMap<String, usize> = struct_table
        .iter()
        .map(|(name, info)| (name.clone(), info.type_id))
        .collect();
    for struct_def in &program.structs {
        if let Some(info) = struct_table.get_mut(&struct_def.name) {
            info.fields = struct_def
                .fields
                .iter()
                .map(|field| {
                    let field_type = field
                        .as_julia_type()
                        .map(|ty| julia_type_to_type_stability_value_type(&ty, &name_to_type_id))
                        .unwrap_or(ValueType::Any);
                    (field.name.clone(), field_type)
                })
                .collect();
        }
    }

    struct_table
}

fn julia_type_to_type_stability_value_type(
    ty: &JuliaType,
    name_to_type_id: &HashMap<String, usize>,
) -> ValueType {
    match ty {
        JuliaType::Int64 => ValueType::I64,
        JuliaType::Int32 => ValueType::I32,
        JuliaType::Int16 => ValueType::I16,
        JuliaType::Int8 => ValueType::I8,
        JuliaType::Int128 => ValueType::I128,
        JuliaType::UInt64 => ValueType::U64,
        JuliaType::UInt32 => ValueType::U32,
        JuliaType::UInt16 => ValueType::U16,
        JuliaType::UInt8 => ValueType::U8,
        JuliaType::UInt128 => ValueType::U128,
        JuliaType::Float64 => ValueType::F64,
        JuliaType::Float32 => ValueType::F32,
        JuliaType::Float16 => ValueType::F16,
        JuliaType::Bool => ValueType::Bool,
        JuliaType::String => ValueType::Str,
        JuliaType::Symbol => ValueType::Symbol,
        JuliaType::Nothing => ValueType::Nothing,
        JuliaType::VectorOf(element) => ValueType::ArrayOf(
            match element.as_ref() {
                JuliaType::Int64 => ArrayElementType::I64,
                JuliaType::Float64 => ArrayElementType::F64,
                JuliaType::Bool => ArrayElementType::Bool,
                JuliaType::String => ArrayElementType::String,
                _ => ArrayElementType::Any,
            },
            None,
        ),
        JuliaType::Struct(name) => name_to_type_id
            .get(name)
            .map(|type_id| ValueType::Struct(*type_id))
            .unwrap_or(ValueType::Any),
        _ => ValueType::Any,
    }
}

fn recover_default_constructor_return_lattice(
    struct_table: &StructRegistry,
    func: &Function,
) -> Option<LatticeType> {
    let expr = match func.body.stmts.as_slice() {
        [Stmt::Return {
            value: Some(expr), ..
        }] => expr,
        [Stmt::Expr { expr, .. }] => expr,
        _ => return None,
    };
    let Expr::Call {
        function,
        args,
        splat_mask,
        kwargs,
        kwargs_splat_mask,
        ..
    } = expr
    else {
        return None;
    };
    if splat_mask.iter().any(|is_splat| *is_splat)
        || kwargs_splat_mask.iter().any(|is_splat| *is_splat)
        || !kwargs.is_empty()
    {
        return None;
    }
    let info = struct_table.get(function.as_str())?;
    if info.has_inner_constructor || info.fields.len() != args.len() {
        return None;
    }
    Some(LatticeType::Concrete(ConcreteType::Struct {
        name: function.to_string(),
        type_id: info.type_id,
    }))
}

fn add_inner_constructor_method_sigs(
    engine: &mut InferenceEngine,
    program: &Program,
    struct_table: &StructRegistry,
    starting_global_index: usize,
) {
    let mut next_global_index = starting_global_index;
    for struct_def in &program.structs {
        let Some(info) = struct_table.get(&struct_def.name) else {
            continue;
        };
        for ctor in &struct_def.inner_constructors {
            let params = ctor
                .params
                .iter()
                .map(|param| (param.name.clone(), param.effective_type()))
                .collect();
            let sig = MethodSig::from_julia_projections(
                0,
                next_global_index,
                params,
                ValueType::Struct(info.type_id),
                Some(JuliaType::Struct(struct_def.name.clone())),
                false,
                ctor.type_params.clone(),
                None,
                None,
            );
            engine.add_initial_method(struct_def.name.clone(), sig);
            next_global_index += 1;
        }
    }
}

impl TypeStabilityAnalyzer {
    /// Creates a new type stability analyzer with default configuration.
    pub fn new() -> Self {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);
        Self {
            config: AnalysisConfig::default(),
            engine: InferenceEngine::new(),
            tfuncs,
        }
    }

    /// Creates a new analyzer with the given configuration.
    pub fn with_config(config: AnalysisConfig) -> Self {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);
        Self {
            config,
            engine: InferenceEngine::new(),
            tfuncs,
        }
    }

    /// Creates a new analyzer with struct table information.
    pub fn with_struct_table(struct_table: HashMap<String, StructTypeInfo>) -> Self {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);
        Self {
            config: AnalysisConfig::default(),
            engine: InferenceEngine::with_struct_table(struct_table),
            tfuncs,
        }
    }

    /// Creates a new analyzer with both struct table and function table.
    pub fn with_tables(
        struct_table: HashMap<String, StructTypeInfo>,
        function_table: HashMap<String, Function>,
    ) -> Self {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);
        Self {
            config: AnalysisConfig::default(),
            engine: InferenceEngine::with_tables(struct_table, function_table),
            tfuncs,
        }
    }

    /// Analyzes a complete program for type stability.
    pub fn analyze_program(&mut self, program: &Program) -> TypeStabilityAnalysisReport {
        let mut report = TypeStabilityAnalysisReport::new();

        let struct_table = build_type_stability_struct_table(program);
        let mut global_types = HashMap::new();
        let mut global_const_structs = HashMap::new();
        collect_global_types_for_inference(
            &program.main.stmts,
            &mut global_types,
            &struct_table,
            &mut global_const_structs,
        );
        let lattice_global_types = global_types
            .iter()
            .map(|(name, ty)| {
                (
                    name.clone(),
                    value_type_to_lattice_with_struct_table(ty, &struct_table),
                )
            })
            .collect();
        self.engine.set_global_types(lattice_global_types);

        // Determine which functions to analyze
        let functions_to_analyze: Vec<&Function> = if self.config.user_functions_only {
            // Only analyze user-defined functions (skip base functions)
            program
                .functions
                .iter()
                .skip(program.base_function_count)
                .map(|f| f.as_ref())
                .collect()
        } else if self.config.include_base_functions {
            // Analyze all functions
            program.functions.iter().map(|f| f.as_ref()).collect()
        } else {
            // Skip internal/generated functions
            program
                .functions
                .iter()
                .map(|f| f.as_ref())
                .filter(|f| !f.name.starts_with('_'))
                .collect()
        };

        let functions_for_inference = Self::collect_program_functions_for_inference(program);

        // Add all functions to the engine for interprocedural analysis
        for func in &functions_for_inference {
            self.engine.add_function(func.clone());
        }
        self.seed_method_tables(&functions_for_inference);

        // Analyze each function
        for func in functions_to_analyze {
            let func_report = self.analyze_function(func);
            report.add_function(func_report);
        }

        report
    }

    /// Analyzes a complete program using return facts produced by the production
    /// shared inference engine. The standalone analyzer remains responsible for
    /// explanatory notes such as usage-inferred parameter constraints.
    pub fn analyze_program_with_production_inference(
        &mut self,
        program: &Program,
    ) -> TypeStabilityAnalysisReport {
        let mut report = TypeStabilityAnalysisReport::new();
        report.inference_provenance = InferenceProvenance::production_shared_inference_snapshot();

        let facts = self.collect_production_inference_facts(program);
        let functions_to_analyze: Vec<(usize, &Function)> = if self.config.user_functions_only {
            program
                .functions
                .iter()
                .enumerate()
                .skip(program.base_function_count)
                .map(|(i, f)| (i, f.as_ref()))
                .collect()
        } else if self.config.include_base_functions {
            program
                .functions
                .iter()
                .enumerate()
                .map(|(i, f)| (i, f.as_ref()))
                .collect()
        } else {
            program
                .functions
                .iter()
                .enumerate()
                .map(|(i, f)| (i, f.as_ref()))
                .filter(|(_, f)| !f.name.starts_with('_'))
                .collect()
        };

        for (function_index, func) in functions_to_analyze {
            let func_report = match facts.return_type_for(function_index) {
                Some(return_type) => self.analyze_function_with_return_type(func, return_type),
                None => self.analyze_function(func),
            };
            report.add_function(func_report);
        }

        report
    }

    fn collect_production_inference_facts(
        &mut self,
        program: &Program,
    ) -> ProductionInferenceFacts {
        let struct_table = build_type_stability_struct_table(program);
        let mut global_types = HashMap::new();
        let mut global_const_structs = HashMap::new();
        collect_global_types_for_inference(
            &program.main.stmts,
            &mut global_types,
            &struct_table,
            &mut global_const_structs,
        );
        widen_non_const_globals_for_binding_inference(&program.main.stmts, &mut global_types);

        let functions_for_inference =
            Self::collect_program_functions_for_production_inference(program);
        let mut engine = crate::compile::inference::build_shared_inference_engine(
            &struct_table,
            &global_types,
            functions_for_inference.iter().map(|(_, func)| func),
        );
        add_inner_constructor_method_sigs(
            &mut engine,
            program,
            &struct_table,
            functions_for_inference.len(),
        );
        let mut facts = ProductionInferenceFacts::default();

        let mut method_sigs = Vec::new();
        for (global_index, (_, func)) in functions_for_inference.iter().enumerate() {
            if !Self::should_collect_production_fact(program, global_index) {
                continue;
            }
            let mut return_lattice = engine.infer_function(func);
            if matches!(return_lattice, LatticeType::Top | LatticeType::Bottom) {
                if let Some(recovered) =
                    recover_default_constructor_return_lattice(&struct_table, func)
                {
                    return_lattice = recovered;
                }
            }
            if global_index < program.functions.len() {
                facts.insert_return_type(global_index, return_lattice.clone());
            }
            method_sigs.push((
                func.name.clone(),
                Self::method_sig_from_return(global_index, func, &return_lattice),
            ));
        }

        // Rebuild with exact user-method return snapshots before collecting the
        // final report facts. This avoids eagerly inferring every Base/prelude
        // function while still giving user overload dispatch the same method
        // return information shape used by production inference (Issue #4291).
        let mut engine = crate::compile::inference::build_shared_inference_engine(
            &struct_table,
            &global_types,
            functions_for_inference.iter().map(|(_, func)| func),
        );
        add_inner_constructor_method_sigs(
            &mut engine,
            program,
            &struct_table,
            functions_for_inference.len(),
        );
        for (name, sig) in method_sigs {
            engine.add_initial_method(name, sig);
        }
        let mut facts = ProductionInferenceFacts::default();
        for (global_index, (_, func)) in functions_for_inference.iter().enumerate() {
            if !Self::should_collect_production_fact(program, global_index) {
                continue;
            }
            let mut return_lattice = engine.infer_function(func);
            if matches!(return_lattice, LatticeType::Top | LatticeType::Bottom) {
                if let Some(recovered) =
                    recover_default_constructor_return_lattice(&struct_table, func)
                {
                    return_lattice = recovered;
                }
            }
            if global_index < program.functions.len() {
                facts.insert_return_type(global_index, return_lattice.clone());
            }
        }

        self.engine = engine;
        facts
    }

    fn should_collect_production_fact(program: &Program, global_index: usize) -> bool {
        global_index < program.functions.len() && global_index >= program.base_function_count
    }

    fn method_sig_from_return(
        global_index: usize,
        func: &Function,
        return_lattice: &LatticeType,
    ) -> MethodSig {
        let return_type = lattice_to_value_type(return_lattice);
        let return_julia_type = lattice_to_parametric_julia_type(return_lattice);
        let vararg_param_index = func.params.iter().position(|param| param.is_varargs);
        let vararg_fixed_count = func
            .params
            .iter()
            .find(|param| param.is_varargs)
            .and_then(|param| param.vararg_count);
        let params = func
            .params
            .iter()
            .map(|param| (param.name.clone(), param.effective_type()))
            .collect();

        MethodSig::from_julia_projections(
            0,
            global_index,
            params,
            return_type,
            return_julia_type,
            func.is_base_extension,
            func.type_params.clone(),
            vararg_param_index,
            vararg_fixed_count,
        )
    }

    fn collect_program_functions_for_inference(program: &Program) -> Vec<Function> {
        let mut functions: Vec<Function> =
            program.functions.iter().map(|f| (**f).clone()).collect();
        let mut nested_functions: Vec<(Function, Option<String>)> = Vec::new();
        for func in &program.functions {
            collect_block_functions(&func.body, &mut nested_functions, Some(&func.name));
        }
        functions.extend(nested_functions.into_iter().map(|(mut func, parent)| {
            if let Some(parent) = parent {
                func.name = format!("{}#{}", parent, func.name);
            }
            func
        }));
        functions
    }

    fn collect_program_functions_for_production_inference(
        program: &Program,
    ) -> Vec<(Option<usize>, Function)> {
        let mut functions: Vec<(Option<usize>, Function)> = program
            .functions
            .iter()
            .map(|f| (**f).clone())
            .enumerate()
            .map(|(index, func)| (Some(index), func))
            .collect();

        let mut nested_functions: Vec<(Function, Option<String>)> = Vec::new();
        for (index, func) in program.functions.iter().enumerate() {
            if index < program.base_function_count {
                continue;
            }
            collect_block_functions(&func.body, &mut nested_functions, Some(&func.name));
        }
        functions.extend(nested_functions.into_iter().map(|(mut func, parent)| {
            if let Some(parent) = parent {
                func.name = format!("{}#{}", parent, func.name);
            }
            (None, func)
        }));
        functions
    }

    fn seed_method_tables(&mut self, functions: &[Function]) {
        for (global_index, func) in functions.iter().enumerate() {
            let return_lattice = self.engine.infer_function(func);
            let return_type = lattice_to_value_type(&return_lattice);
            let return_julia_type = lattice_to_parametric_julia_type(&return_lattice);
            let vararg_param_index = func.params.iter().position(|param| param.is_varargs);
            let vararg_fixed_count = func
                .params
                .iter()
                .find(|param| param.is_varargs)
                .and_then(|param| param.vararg_count);
            let params = func
                .params
                .iter()
                .map(|param| (param.name.clone(), param.effective_type()))
                .collect();

            let sig = MethodSig::from_julia_projections(
                0,
                global_index,
                params,
                return_type,
                return_julia_type,
                func.is_base_extension,
                func.type_params.clone(),
                vararg_param_index,
                vararg_fixed_count,
            );
            self.engine.add_method(func.name.clone(), sig);
        }
    }

    /// Analyzes a single function for type stability.
    pub fn analyze_function(&mut self, func: &Function) -> FunctionStabilityReport {
        let return_type = self.engine.infer_function(func);
        self.analyze_function_with_return_type(func, &return_type)
    }

    fn analyze_function_with_return_type(
        &mut self,
        func: &Function,
        return_type: &LatticeType,
    ) -> FunctionStabilityReport {
        // Use usage analysis to infer parameter constraints for untyped parameters
        let usage_constraints = usage_analysis::infer_parameter_constraints(func, &self.tfuncs);

        // Track which parameters were inferred (for informational reporting)
        let mut inferred_params: Vec<(String, String)> = Vec::new();

        // Build input signature from parameters
        let input_signature: Vec<(String, LatticeType)> = func
            .params
            .iter()
            .map(|param| {
                let param_type = if let Some(ref ty) = param.type_annotation {
                    self.julia_type_to_lattice(ty)
                } else {
                    // Use usage-based inference for untyped parameters
                    let inferred = usage_constraints
                        .get(&param.name)
                        .cloned()
                        .unwrap_or(LatticeType::Top);

                    // Track non-Top inferred types for reporting
                    if inferred != LatticeType::Top {
                        inferred_params
                            .push((param.name.clone(), Self::format_lattice_type(&inferred)));
                    }

                    inferred
                };
                (param.name.clone(), param_type)
            })
            .collect();

        // Use source line, not byte offset, in user-facing diagnostics.
        let line = func.span.start_line;

        // Create the report
        let mut report = FunctionStabilityReport::new(
            func.name.clone(),
            line,
            input_signature.clone(),
            return_type.clone(),
        );

        // Add informational note about inferred parameter types
        if !inferred_params.is_empty() {
            report.add_reason(TypeStabilityReason::InferredParameterTypes {
                inferred: inferred_params,
            });
        }

        // Add reasons for instability
        self.analyze_instability_reasons(&mut report, func, return_type, &input_signature);

        report
    }

    /// Analyzes and adds reasons for type instability.
    fn analyze_instability_reasons(
        &self,
        report: &mut FunctionStabilityReport,
        _func: &Function,
        return_type: &LatticeType,
        input_signature: &[(String, LatticeType)],
    ) {
        // Check return type stability
        match return_type {
            LatticeType::Top => {
                report.add_reason(TypeStabilityReason::ReturnsTop);
            }
            LatticeType::Union(types) => {
                report.add_reason(TypeStabilityReason::ReturnsUnion {
                    types: types.clone(),
                });
            }
            LatticeType::Conditional { .. } => {
                report.add_reason(TypeStabilityReason::ConditionalBranchMismatch {
                    then_type: "varies".to_string(),
                    else_type: "varies".to_string(),
                });
            }
            _ => {}
        }

        // Check for untyped parameters (if strict mode)
        if self.config.strict_parameter_typing {
            let untyped: Vec<String> = input_signature
                .iter()
                .filter(|(_, ty)| *ty == LatticeType::Top)
                .map(|(name, _)| name.clone())
                .collect();

            if !untyped.is_empty() {
                report.add_reason(TypeStabilityReason::UntypedParameters {
                    param_names: untyped,
                });
            }
        }
    }

    /// Converts a Julia type annotation to a LatticeType.
    ///
    /// Issue #5916: delegates to the canonical
    /// [`crate::runtime_types::bridge::julia_type_to_lattice`] (table-free: a
    /// `JuliaType::Struct` keeps the `type_id: 0` placeholder) so the
    /// stability analyzer and the compiler bridge cannot drift apart on
    /// annotation lowering.
    fn julia_type_to_lattice(&self, ty: &crate::types::JuliaType) -> LatticeType {
        crate::runtime_types::bridge::julia_type_to_lattice(ty)
    }
}

impl TypeStabilityAnalyzer {
    /// Formats a LatticeType for display (used in reports).
    fn format_lattice_type(ty: &LatticeType) -> String {
        match ty {
            LatticeType::Bottom => "Bottom".to_string(),
            LatticeType::Const(cv) => format!("Const({:?})", cv),
            LatticeType::Concrete(ct) => format!("{:?}", ct),
            LatticeType::Union(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
                format!("Union{{{}}}", type_strs.join(", "))
            }
            LatticeType::Conditional { .. } => "Conditional".to_string(),
            LatticeType::Top => "Any".to_string(),
            // Display as the widened struct name; the field facts are an
            // inference-internal refinement (Issue #8544).
            LatticeType::PartialStruct { struct_name, .. } => struct_name.clone(),
        }
    }
}

impl Default for TypeStabilityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::inference_core::{CoreAbstract, CorePrimitive, CoreType};
    use crate::ir::core::{Block, Expr, Literal, Program, Stmt, TypedParam};
    use crate::span::Span;
    use crate::types::JuliaType;

    fn create_test_function(name: &str, params: Vec<TypedParam>, body: Block) -> Function {
        Function {
            name: name.to_string(),
            params,
            kwparams: vec![],
            type_params: vec![],
            body,
            return_type: None,
            is_base_extension: false,
            is_runtime_eval: false,
            span: create_span(),
            new_struct_name: None,
        }
    }

    fn create_span() -> Span {
        Span::new(0, 10, 1, 1, 0, 10)
    }

    fn empty_block() -> Block {
        Block {
            stmts: vec![],
            span: create_span(),
        }
    }

    #[test]
    fn test_stable_int_function() {
        let func = create_test_function(
            "double",
            vec![TypedParam::new(
                "x".to_string(),
                Some(JuliaType::Int64),
                create_span(),
            )],
            Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: crate::ir::core::BinaryOp::Mul,
                        left: Box::new(Expr::Var("x".to_string().into(), create_span())),
                        right: Box::new(Expr::Literal(Literal::Int(2), create_span())),
                        span: create_span(),
                    }),
                    span: create_span(),
                }],
                span: create_span(),
            },
        );

        let mut analyzer = TypeStabilityAnalyzer::new();
        let report = analyzer.analyze_function(&func);

        assert_eq!(report.function_name, "double");
        assert!(report.is_stable());
    }

    #[test]
    fn test_unstable_untyped_function() {
        let func = create_test_function(
            "identity",
            vec![TypedParam::new(
                "x".to_string(),
                None, // Untyped parameter
                create_span(),
            )],
            Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Var("x".to_string().into(), create_span())),
                    span: create_span(),
                }],
                span: create_span(),
            },
        );

        let mut analyzer = TypeStabilityAnalyzer::new();
        let report = analyzer.analyze_function(&func);

        // Without type annotation, the function returns Top (Any)
        assert!(report.is_unstable());
    }

    #[test]
    fn test_analyzer_config() {
        let config = AnalysisConfig {
            include_base_functions: true,
            user_functions_only: false,
            strict_parameter_typing: true,
        };

        let analyzer = TypeStabilityAnalyzer::with_config(config.clone());
        assert!(analyzer.config.include_base_functions);
        assert!(!analyzer.config.user_functions_only);
        assert!(analyzer.config.strict_parameter_typing);
    }

    #[test]
    fn test_program_analysis_uses_method_table_dispatch() {
        let int_method = create_test_function(
            "f4291",
            vec![TypedParam::new(
                "x".to_string(),
                Some(JuliaType::Integer),
                create_span(),
            )],
            Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Literal(Literal::Int(1), create_span())),
                    span: create_span(),
                }],
                span: create_span(),
            },
        );
        let number_method = create_test_function(
            "f4291",
            vec![TypedParam::new(
                "x".to_string(),
                Some(JuliaType::Number),
                create_span(),
            )],
            Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Literal(Literal::Float(1.0), create_span())),
                    span: create_span(),
                }],
                span: create_span(),
            },
        );
        let caller = create_test_function(
            "caller4291",
            vec![TypedParam::new(
                "x".to_string(),
                Some(JuliaType::Int64),
                create_span(),
            )],
            Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "f4291".to_string().into(),
                        args: vec![Expr::Var("x".to_string().into(), create_span())],
                        kwargs: vec![],
                        splat_mask: vec![false],
                        kwargs_splat_mask: vec![],
                        span: create_span(),
                    }),
                    span: create_span(),
                }],
                span: create_span(),
            },
        );

        let program = Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            structs: vec![],
            functions: vec![
                std::sync::Arc::new(int_method),
                std::sync::Arc::new(number_method),
                std::sync::Arc::new(caller),
            ],
            base_function_count: 0,
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: empty_block(),
        };

        let mut analyzer = TypeStabilityAnalyzer::new();
        let report = analyzer.analyze_program(&program);
        let caller_report = report
            .functions
            .iter()
            .find(|function| function.function_name == "caller4291")
            .expect("caller4291 report should exist");

        assert!(
            caller_report.is_stable(),
            "expected method table dispatch to infer caller4291, got {caller_report:?}"
        );
        assert_eq!(
            caller_report.return_type,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_usage_analysis_infers_numeric_type() {
        // function add_one(x) return x + 1 end
        // Usage analysis should infer x as numeric (Union{Int64, Float64})
        let func = create_test_function(
            "add_one",
            vec![TypedParam::new(
                "x".to_string(),
                None, // Untyped parameter
                create_span(),
            )],
            Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: crate::ir::core::BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string().into(), create_span())),
                        right: Box::new(Expr::Literal(Literal::Int(1), create_span())),
                        span: create_span(),
                    }),
                    span: create_span(),
                }],
                span: create_span(),
            },
        );

        let mut analyzer = TypeStabilityAnalyzer::new();
        let report = analyzer.analyze_function(&func);

        // Check that usage analysis inferred a type for x
        // The report should contain InferredParameterTypes reason
        let has_inferred_reason = report.reasons.iter().any(|r| {
            matches!(r, TypeStabilityReason::InferredParameterTypes { inferred } if !inferred.is_empty())
        });
        assert!(
            has_inferred_reason,
            "Expected InferredParameterTypes reason, got: {:?}",
            report.reasons
        );

        // Check that x is now Number (abstract numeric type), not Top
        let x_type = report
            .input_signature
            .iter()
            .find(|(name, _)| name == "x")
            .map(|(_, ty)| ty);
        assert!(
            matches!(
                x_type,
                Some(LatticeType::Concrete(ConcreteType::Core(
                    CoreType::Abstract(CoreAbstract::Number)
                )))
            ),
            "Expected x to be inferred as Number, got: {:?}",
            x_type
        );
    }

    #[test]
    fn test_usage_analysis_infers_integer_from_indexing() {
        // function get_elem(arr, i) return arr[i] end
        // Usage analysis should infer i as Int64 (used as array index)
        let func = create_test_function(
            "get_elem",
            vec![
                TypedParam::new("arr".to_string(), None, create_span()),
                TypedParam::new("i".to_string(), None, create_span()),
            ],
            Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Index {
                        array: Box::new(Expr::Var("arr".to_string().into(), create_span())),
                        indices: vec![Expr::Var("i".to_string().into(), create_span())],
                        span: create_span(),
                    }),
                    span: create_span(),
                }],
                span: create_span(),
            },
        );

        let mut analyzer = TypeStabilityAnalyzer::new();
        let report = analyzer.analyze_function(&func);

        // Check that usage analysis inferred Int64 for i
        let i_type = report
            .input_signature
            .iter()
            .find(|(name, _)| name == "i")
            .map(|(_, ty)| ty);
        assert!(
            matches!(
                i_type,
                Some(LatticeType::Concrete(ConcreteType::Core(
                    CoreType::Primitive(CorePrimitive::Int64)
                )))
            ),
            "Expected i to be inferred as Int64, got: {:?}",
            i_type
        );
    }
}
