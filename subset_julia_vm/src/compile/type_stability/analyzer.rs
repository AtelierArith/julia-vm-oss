//! Type stability analyzer.
//!
//! This module implements the core analysis logic for checking type stability.

use crate::compile::abstract_interp::{usage_analysis, InferenceEngine, StructTypeInfo};
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::tfuncs::TransferFunctions;
use crate::compile::type_stability::analysis_report::TypeStabilityAnalysisReport;
use crate::compile::type_stability::reason::TypeStabilityReason;
use crate::compile::type_stability::report::FunctionStabilityReport;
use crate::ir::core::{Function, Program};
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

        // Determine which functions to analyze
        let functions_to_analyze: Vec<&Function> = if self.config.user_functions_only {
            // Only analyze user-defined functions (skip base functions)
            program
                .functions
                .iter()
                .skip(program.base_function_count)
                .collect()
        } else if self.config.include_base_functions {
            // Analyze all functions
            program.functions.iter().collect()
        } else {
            // Skip internal/generated functions
            program
                .functions
                .iter()
                .filter(|f| !f.name.starts_with('_'))
                .collect()
        };

        // Add all functions to the engine for interprocedural analysis
        for func in &program.functions {
            self.engine.add_function(func.clone());
        }

        // Analyze each function
        for func in functions_to_analyze {
            let func_report = self.analyze_function(func);
            report.add_function(func_report);
        }

        report
    }

    /// Analyzes a single function for type stability.
    pub fn analyze_function(&mut self, func: &Function) -> FunctionStabilityReport {
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

        // Infer the return type
        let return_type = self.engine.infer_function(func);

        // Get line number from span
        let line = func.body.span.start;

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
        self.analyze_instability_reasons(&mut report, func, &return_type, &input_signature);

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

    /// Converts a Julia type to a LatticeType.
    fn julia_type_to_lattice(&self, ty: &crate::types::JuliaType) -> LatticeType {
        use crate::types::JuliaType;

        match ty {
            // Signed integers
            JuliaType::Int8 => LatticeType::Concrete(ConcreteType::Int8),
            JuliaType::Int16 => LatticeType::Concrete(ConcreteType::Int16),
            JuliaType::Int32 => LatticeType::Concrete(ConcreteType::Int32),
            JuliaType::Int64 => LatticeType::Concrete(ConcreteType::Int64),
            JuliaType::Int128 => LatticeType::Concrete(ConcreteType::Int128),
            JuliaType::BigInt => LatticeType::Concrete(ConcreteType::BigInt),
            // Unsigned integers
            JuliaType::UInt8 => LatticeType::Concrete(ConcreteType::UInt8),
            JuliaType::UInt16 => LatticeType::Concrete(ConcreteType::UInt16),
            JuliaType::UInt32 => LatticeType::Concrete(ConcreteType::UInt32),
            JuliaType::UInt64 => LatticeType::Concrete(ConcreteType::UInt64),
            JuliaType::UInt128 => LatticeType::Concrete(ConcreteType::UInt128),
            // Floating point
            JuliaType::Float16 => LatticeType::Concrete(ConcreteType::Float16),
            JuliaType::Float32 => LatticeType::Concrete(ConcreteType::Float32),
            JuliaType::Float64 => LatticeType::Concrete(ConcreteType::Float64),
            JuliaType::BigFloat => LatticeType::Concrete(ConcreteType::BigFloat),
            // Other primitives
            JuliaType::Bool => LatticeType::Concrete(ConcreteType::Bool),
            JuliaType::String => LatticeType::Concrete(ConcreteType::String),
            JuliaType::Char => LatticeType::Concrete(ConcreteType::Char),
            JuliaType::Nothing => LatticeType::Concrete(ConcreteType::Nothing),
            JuliaType::Missing => LatticeType::Concrete(ConcreteType::Missing),
            JuliaType::Symbol => LatticeType::Concrete(ConcreteType::Symbol),
            // Array types
            JuliaType::Array => LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Any),
            }),
            JuliaType::VectorOf(elem) => {
                let elem_concrete =
                    if let LatticeType::Concrete(ct) = self.julia_type_to_lattice(elem) {
                        ct
                    } else {
                        ConcreteType::Any
                    };
                LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(elem_concrete),
                })
            }
            JuliaType::MatrixOf(elem) => {
                let elem_concrete =
                    if let LatticeType::Concrete(ct) = self.julia_type_to_lattice(elem) {
                        ct
                    } else {
                        ConcreteType::Any
                    };
                LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(elem_concrete),
                })
            }
            // Tuple types
            JuliaType::Tuple => LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] }),
            JuliaType::TupleOf(elems) => {
                let element_types: Vec<ConcreteType> = elems
                    .iter()
                    .filter_map(|t| {
                        if let LatticeType::Concrete(ct) = self.julia_type_to_lattice(t) {
                            Some(ct)
                        } else {
                            None
                        }
                    })
                    .collect();
                LatticeType::Concrete(ConcreteType::Tuple {
                    elements: element_types,
                })
            }
            // Dict and Set
            JuliaType::Dict => LatticeType::Concrete(ConcreteType::Dict {
                key: Box::new(ConcreteType::Any),
                value: Box::new(ConcreteType::Any),
            }),
            JuliaType::Set => LatticeType::Concrete(ConcreteType::Set {
                element: Box::new(ConcreteType::Any),
            }),
            // Range types
            JuliaType::UnitRange | JuliaType::StepRange => {
                LatticeType::Concrete(ConcreteType::Range {
                    element: Box::new(ConcreteType::Int64),
                })
            }
            // User-defined struct
            JuliaType::Struct(name) => LatticeType::Concrete(ConcreteType::Struct {
                name: name.clone(),
                type_id: 0, // Type ID resolved later
            }),
            // Any type
            JuliaType::Any => LatticeType::Top,
            // Fallback: other JuliaType variants mapped to Top
            _ => LatticeType::Top,
        }
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
        }
    }
}

impl Default for TypeStabilityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Expr, Literal, Stmt, TypedParam};
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
            span: create_span(),
        }
    }

    fn create_span() -> Span {
        Span::new(0, 10, 1, 1, 0, 10)
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
                        left: Box::new(Expr::Var("x".to_string(), create_span())),
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
                    value: Some(Expr::Var("x".to_string(), create_span())),
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
                        left: Box::new(Expr::Var("x".to_string(), create_span())),
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
            matches!(x_type, Some(LatticeType::Concrete(ConcreteType::Number))),
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
                        array: Box::new(Expr::Var("arr".to_string(), create_span())),
                        indices: vec![Expr::Var("i".to_string(), create_span())],
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
            matches!(i_type, Some(LatticeType::Concrete(ConcreteType::Int64))),
            "Expected i to be inferred as Int64, got: {:?}",
            i_type
        );
    }
}
