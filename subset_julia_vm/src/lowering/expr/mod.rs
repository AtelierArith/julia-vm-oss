//! Expression lowering.
//!
//! This module handles lowering of CST expressions to Core IR.

mod binary;
mod call;
mod collection;
mod helpers;
mod literal;
mod macros;
mod misc;
mod quote;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};

// Re-export helpers for use in submodules
pub(super) use helpers::{
    is_broadcast_op, is_comparison_operator, is_flattenable_operator, is_operator_token,
    make_broadcasted_call, map_binary_op, map_builtin_name, map_unary_op,
    process_raw_string_escapes, strip_broadcast_dot,
};

// Re-export macro functions for submodules
pub(super) use macros::{lower_macro_expr, lower_macro_expr_with_ctx};

// Re-export quote functions
pub(super) use quote::lower_quote_expr;
pub use quote::{
    quote_constructor_to_code, quote_constructor_to_code_with_locals,
    quote_constructor_to_code_with_varargs,
};

// Re-export public functions
pub use collection::extract_index_target;
pub use misc::extract_field_target;
pub use misc::extract_nested_field_target;
pub use misc::extract_nested_field_target_with_ctx;

// Re-export for submodules
pub(super) use binary::{
    lower_binary_expr, lower_binary_expr_with_ctx, lower_juxtaposition_expr, lower_unary_expr,
};
pub(super) use call::{
    lower_argument_list, lower_arrow_function, lower_call_expr, lower_call_expr_with_ctx,
};
pub(super) use collection::{
    lower_comprehension_expr, lower_generator_expr, lower_index_expr, lower_matrix_expr,
    lower_range_expr, lower_vector_expr,
};
pub(super) use literal::{lower_char_literal, lower_string_literal, parse_float, parse_int};
pub(super) use misc::{
    lower_adjoint_expr, lower_broadcast_call_expr, lower_field_expr, lower_if_expr, lower_let_expr,
    lower_pair_expr, lower_parenthesized_expr, lower_parenthesized_expr_with_ctx,
    lower_ternary_expr, lower_tuple_expr,
};

/// Main expression lowering function.
/// Dispatches to specialized handlers based on node kind.
///
/// # Design Rule: Do NOT hardcode prelude constants here (Issue #2866)
///
/// Only the following identifiers are special-cased at the lowering stage:
/// - **Julia keywords / literals**: `true`, `false`, `nothing`, `missing`
/// - **Built-in module references**: `Base`, `Core`, `Main`, `Meta`
///
/// All other constants — including `im`, `pi`, `ℯ`, `Inf`, `NaN`, etc. —
/// are defined in `subset_julia_vm/src/julia/base/` and must fall through to
/// `Expr::Var` so the compiler resolves them from `global_types` at compile time.
///
/// **Do NOT add** `"im" => ...`, `"pi" => ...`, or similar here. If you need
/// compile-time type information for a constant, add it to the type inference
/// layer (`compile/expr/infer/`) with a `// Workaround: (Issue #XXXX):` comment
/// explaining when it can be removed.
pub fn lower_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    match walker.kind(&node) {
        NodeKind::Identifier => {
            let name = walker.text(&node);
            match name {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                "nothing" => Ok(Expr::Literal(Literal::Nothing, span)),
                "missing" => Ok(Expr::Literal(Literal::Missing, span)),
                // Built-in module values
                "Base" => Ok(Expr::Literal(Literal::Module("Base".to_string()), span)),
                "Core" => Ok(Expr::Literal(Literal::Module("Core".to_string()), span)),
                "Main" => Ok(Expr::Literal(Literal::Module("Main".to_string()), span)),
                // Meta is a submodule of Base, accessible as just "Meta"
                "Meta" => Ok(Expr::Literal(Literal::Module("Meta".to_string()), span)),
                // Note: Don't convert "pi" here - let compiler decide based on whether
                // it's a local variable. This allows `for pi in 1:10` to work.
                // Note: Don't convert "im" here either — it is defined in the prelude
                // as `const im = Complex{Bool}(false, true)`. The compiler resolves
                // its type via type inference (see compile/expr/infer/).
                _ => Ok(Expr::Var(name.to_string(), span)),
            }
        }
        NodeKind::IntegerLiteral => {
            let text = walker.text(&node);
            let parsed = parse_int(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match parsed {
                literal::ParsedInt::I64(v) => Ok(Expr::Literal(Literal::Int(v), span)),
                literal::ParsedInt::I128(v) => Ok(Expr::Literal(Literal::Int128(v), span)),
                literal::ParsedInt::BigInt(v) => Ok(Expr::Literal(Literal::BigInt(v), span)),
            }
        }
        NodeKind::FloatLiteral => {
            let text = walker.text(&node);
            let parsed = parse_float(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match parsed {
                literal::ParsedFloat::F64(v) => Ok(Expr::Literal(Literal::Float(v), span)),
                literal::ParsedFloat::F32(v) => Ok(Expr::Literal(Literal::Float32(v), span)),
            }
        }
        NodeKind::StringLiteral => lower_string_literal(walker, node, None),
        NodeKind::CharacterLiteral => lower_char_literal(walker, node),
        NodeKind::BooleanLiteral => {
            let text = walker.text(&node);
            match text {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                _ => Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "boolean literal: {}",
                        text
                    )),
                    span,
                )),
            }
        }
        NodeKind::BinaryExpression => lower_binary_expr(walker, node),
        NodeKind::UnaryExpression => lower_unary_expr(walker, node),
        NodeKind::JuxtapositionExpression => lower_juxtaposition_expr(walker, node),
        NodeKind::CallExpression => lower_call_expr(walker, node),
        NodeKind::ParenthesizedExpression => lower_parenthesized_expr(walker, node),
        NodeKind::RangeExpression => lower_range_expr(walker, node),
        NodeKind::VectorExpression => lower_vector_expr(walker, node),
        NodeKind::MatrixExpression => lower_matrix_expr(walker, node),
        NodeKind::IndexExpression => lower_index_expr(walker, node),
        NodeKind::ComprehensionExpression => lower_comprehension_expr(walker, node),
        NodeKind::GeneratorExpression => lower_generator_expr(walker, node),
        NodeKind::FieldExpression => lower_field_expr(walker, node),
        NodeKind::AdjointExpression => lower_adjoint_expr(walker, node),
        NodeKind::TupleExpression => lower_tuple_expr(walker, node),
        NodeKind::BroadcastCallExpression => lower_broadcast_call_expr(walker, node),
        NodeKind::LetStatement | NodeKind::LetExpression => lower_let_expr(walker, node),
        NodeKind::TernaryExpression => lower_ternary_expr(walker, node),
        NodeKind::IfStatement => lower_if_expr(walker, node),
        NodeKind::PairExpression => lower_pair_expr(walker, node),
        NodeKind::ParametrizedTypeExpression => {
            // Parametric type as expression: Complex{Float64}, Vector{Int64}, etc.
            // In Julia, this evaluates to the Type itself (a DataType value).
            //
            // For static types (all type args are concrete type names), we emit TypeOf.
            // For dynamic types (type args contain function calls or type variables),
            // we emit DynamicTypeConstruct which evaluates type args at runtime.
            lower_parametrized_type_expr(walker, node)
        }
        NodeKind::MacroCall => lower_macro_expr(walker, node),
        NodeKind::QuoteExpression => lower_quote_expr(walker, node),
        // Handle begin...end blocks as expressions (Issue #1794)
        NodeKind::Block => {
            // Lower the block contents as statements
            let children = walker.named_children(&node);
            let mut stmts = Vec::new();
            for child in children {
                let stmt = crate::lowering::stmt::lower_stmt(walker, child)?;
                stmts.push(stmt);
            }
            // Wrap in a LetBlock expression
            let body = crate::ir::core::Block { stmts, span };
            Ok(Expr::LetBlock {
                bindings: vec![],
                body,
                span,
            })
        }
        // Handle prefixed string literals (r"...", raw"...", big"...", etc.)
        // These are mapped to NodeKind::Other but have a specific raw kind
        _ if node.kind() == "prefixed_string_literal" => {
            // PrefixedStringLiteral has two children: [prefix, string]
            let children = walker.named_children(&node);
            if children.len() >= 2 {
                let prefix_text = walker.text(&children[0]);
                let string_text = walker.text(&children[1]);
                // Remove quotes from the string content
                let content = string_text.trim_matches('"').to_string();

                match prefix_text {
                    "big" => {
                        // big"..." creates BigInt or BigFloat depending on content
                        // If content contains '.' or 'e'/'E' (scientific notation), it's BigFloat
                        if content.contains('.') || content.contains('e') || content.contains('E') {
                            Ok(Expr::Literal(Literal::BigFloat(content), span))
                        } else {
                            Ok(Expr::Literal(Literal::BigInt(content), span))
                        }
                    }
                    "raw" => {
                        // raw"..." creates a raw string literal
                        // In Julia, raw strings still process \\ (to \) and \" (to ")
                        // but all other escape sequences are kept as-is
                        let processed = process_raw_string_escapes(&content);
                        Ok(Expr::Literal(Literal::Str(processed), span))
                    }
                    "r" => {
                        // r"..." is a regex literal in Julia
                        // Create a Regex literal with the pattern
                        Ok(Expr::Literal(
                            Literal::Regex {
                                pattern: content,
                                flags: String::new(),
                            },
                            span,
                        ))
                    }
                    "MIME" => {
                        // MIME"text/plain" -> _mime_construct("text/plain")
                        // This creates a MIME{Symbol("text/plain")} type instance
                        Ok(Expr::Call {
                            function: "_mime_construct".to_string(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "v" => {
                        // v"1.2.3" creates a VersionNumber
                        // Parse version string and create constructor call
                        let parts: Vec<&str> = content.split('.').collect();
                        let major = parts
                            .first()
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let minor = parts
                            .get(1)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let patch = parts
                            .get(2)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        Ok(Expr::Call {
                            function: "VersionNumber".to_string(),
                            args: vec![
                                Expr::Literal(Literal::Int(major), span),
                                Expr::Literal(Literal::Int(minor), span),
                                Expr::Literal(Literal::Int(patch), span),
                            ],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "b" => {
                        // b"data" creates a byte array (Vector{UInt8})
                        // Convert string to array of UInt8 values
                        let bytes: Vec<Expr> = content
                            .bytes()
                            .map(|b| Expr::Literal(Literal::Int(b as i64), span))
                            .collect();
                        let len = bytes.len();
                        Ok(Expr::ArrayLiteral {
                            elements: bytes,
                            shape: vec![len],
                            span,
                        })
                    }
                    "Int128" => {
                        // Int128"123" creates an Int128 literal
                        if let Ok(val) = content.parse::<i128>() {
                            Ok(Expr::Literal(Literal::Int128(val), span))
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid Int128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "UInt128" => {
                        // UInt128"123" creates a UInt128 literal
                        // Note: We store as i128 but the type system will treat it as UInt128
                        if let Ok(val) = content.parse::<u128>() {
                            // Convert to i128 for storage (bit pattern preservation)
                            Ok(Expr::Literal(Literal::Int128(val as i128), span))
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid UInt128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "s" => {
                        // s"..." creates a SubstitutionString (used in replace)
                        // For now, just return as regular string
                        Ok(Expr::Literal(Literal::Str(content), span))
                    }
                    _ => {
                        // Unknown prefixes are string macros in Julia:
                        // prefix"text" → @prefix_str("text")
                        // e.g., html"<b>bold</b>" → @html_str("<b>bold</b>")
                        //       L"x^2" → @L_str("x^2")
                        let macro_name = format!("{}_str", prefix_text);
                        Ok(Expr::Call {
                            function: macro_name,
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                }
            } else {
                // Fallback: treat as regular string literal
                let text = walker.text(&node);
                Ok(Expr::Literal(Literal::Str(text.to_string()), span))
            }
        }
        // Jump expressions: used in short-circuit context like `cond && return x`
        NodeKind::ReturnStatement => {
            let children = walker.named_children(&node);
            let value = if children.is_empty() {
                None
            } else {
                Some(Box::new(lower_expr(walker, children[0])?))
            };
            Ok(Expr::ReturnExpr { value, span })
        }
        NodeKind::BreakStatement => Ok(Expr::BreakExpr { span }),
        NodeKind::ContinueStatement => Ok(Expr::ContinueExpr { span }),
        // Handle typed expressions like Int64[] (typed empty array)
        NodeKind::TypedExpression => {
            let children = walker.named_children(&node);
            if children.len() == 2 {
                let type_node = children[0];
                let value_node = children[1];

                // Check if this is a typed empty array: Type[]
                if walker.kind(&type_node) == NodeKind::Identifier
                    && walker.kind(&value_node) == NodeKind::VectorExpression
                {
                    let type_children = walker.named_children(&value_node);
                    if type_children.is_empty() {
                        // Int64[] or similar: typed empty array
                        let element_type = walker.text(&type_node).to_string();
                        return Ok(Expr::TypedEmptyArray { element_type, span });
                    }
                }
            }
            // Fall through to error for unsupported typed expressions
            Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(format!(
                    "typed_expression: {}",
                    walker.text(&node)
                )),
                span,
            ))
        }
        // Bare operator as expression: treat as function reference (Issue #1985)
        // e.g., f = +; map(f, ...) or passing operators to higher-order functions
        NodeKind::Operator => {
            let op_text = walker.text(&node).to_string();
            Ok(Expr::FunctionRef {
                name: op_text,
                span,
            })
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(node.kind().to_string()),
            span,
        )),
    }
}

/// Lower expression with lambda context (for use within function bodies).
pub fn lower_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    match walker.kind(&node) {
        NodeKind::ArrowFunctionExpression => lower_arrow_function(walker, node, lambda_ctx),
        NodeKind::CallExpression => lower_call_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::Identifier => {
            let name = walker.text(&node);
            match name {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                "nothing" => Ok(Expr::Literal(Literal::Nothing, span)),
                "missing" => Ok(Expr::Literal(Literal::Missing, span)),
                // Built-in module values
                "Base" => Ok(Expr::Literal(Literal::Module("Base".to_string()), span)),
                "Core" => Ok(Expr::Literal(Literal::Module("Core".to_string()), span)),
                "Main" => Ok(Expr::Literal(Literal::Module("Main".to_string()), span)),
                // Meta is a submodule of Base, accessible as just "Meta"
                "Meta" => Ok(Expr::Literal(Literal::Module("Meta".to_string()), span)),
                _ => Ok(Expr::Var(name.to_string(), span)),
            }
        }
        NodeKind::IntegerLiteral => {
            let text = walker.text(&node);
            let parsed = parse_int(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match parsed {
                literal::ParsedInt::I64(v) => Ok(Expr::Literal(Literal::Int(v), span)),
                literal::ParsedInt::I128(v) => Ok(Expr::Literal(Literal::Int128(v), span)),
                literal::ParsedInt::BigInt(v) => Ok(Expr::Literal(Literal::BigInt(v), span)),
            }
        }
        NodeKind::FloatLiteral => {
            let text = walker.text(&node);
            let parsed = parse_float(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match parsed {
                literal::ParsedFloat::F64(v) => Ok(Expr::Literal(Literal::Float(v), span)),
                literal::ParsedFloat::F32(v) => Ok(Expr::Literal(Literal::Float32(v), span)),
            }
        }
        NodeKind::StringLiteral => lower_string_literal(walker, node, Some(lambda_ctx)),
        NodeKind::CharacterLiteral => lower_char_literal(walker, node),
        NodeKind::BooleanLiteral => {
            let text = walker.text(&node);
            match text {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                _ => Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "boolean literal: {}",
                        text
                    )),
                    span,
                )),
            }
        }
        NodeKind::BinaryExpression => lower_binary_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::UnaryExpression => lower_unary_expr(walker, node),
        NodeKind::JuxtapositionExpression => lower_juxtaposition_expr(walker, node),
        NodeKind::ParenthesizedExpression => {
            lower_parenthesized_expr_with_ctx(walker, node, lambda_ctx)
        }
        NodeKind::RangeExpression => lower_range_expr(walker, node),
        NodeKind::VectorExpression => lower_vector_expr(walker, node),
        NodeKind::MatrixExpression => lower_matrix_expr(walker, node),
        NodeKind::IndexExpression => lower_index_expr(walker, node),
        NodeKind::ComprehensionExpression => lower_comprehension_expr(walker, node),
        NodeKind::GeneratorExpression => lower_generator_expr(walker, node),
        NodeKind::FieldExpression => lower_field_expr(walker, node),
        NodeKind::AdjointExpression => lower_adjoint_expr(walker, node),
        NodeKind::TupleExpression => lower_tuple_expr(walker, node),
        NodeKind::BroadcastCallExpression => lower_broadcast_call_expr(walker, node),
        NodeKind::LetStatement | NodeKind::LetExpression => lower_let_expr(walker, node),
        NodeKind::TernaryExpression => lower_ternary_expr(walker, node),
        NodeKind::IfStatement => lower_if_expr(walker, node),
        NodeKind::PairExpression => lower_pair_expr(walker, node),
        NodeKind::ParametrizedTypeExpression => {
            // Parametric type as expression: Complex{Float64}, Vector{Int64}, etc.
            // In Julia, this evaluates to the Type itself (a DataType value).
            let name = walker.text(&node).to_string();
            Ok(Expr::Builtin {
                name: crate::ir::core::BuiltinOp::TypeOf,
                args: vec![Expr::Literal(crate::ir::core::Literal::Str(name), span)],
                span,
            })
        }
        NodeKind::MacroCall => lower_macro_expr_with_ctx(walker, node, lambda_ctx),
        NodeKind::QuoteExpression => lower_quote_expr(walker, node),
        // Handle begin...end blocks as expressions
        NodeKind::Block => {
            // Lower the block contents as statements
            let children = walker.named_children(&node);
            let mut stmts = Vec::new();
            for child in children {
                let stmt = crate::lowering::stmt::lower_stmt_with_ctx(walker, child, lambda_ctx)?;
                stmts.push(stmt);
            }
            // Wrap in a LetBlock expression
            let body = crate::ir::core::Block { stmts, span };
            Ok(Expr::LetBlock {
                bindings: vec![],
                body,
                span,
            })
        }
        // Assignment as expression: x = value evaluates to value and assigns it to x
        // This is used for chained assignments like `local result = x = 42`
        NodeKind::Assignment => {
            let children = walker.named_children(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "assignment expression with insufficient children".to_string(),
                    ),
                    span,
                ));
            }
            // First child is target (variable name), last child is value
            let target = &children[0];
            let value = &children[children.len() - 1];

            if walker.kind(target) != NodeKind::Identifier {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "assignment target must be identifier, got {:?}",
                        walker.kind(target)
                    )),
                    span,
                ));
            }

            let var_name = walker.text(target).to_string();
            let value_expr = lower_expr_with_ctx(walker, *value, lambda_ctx)?;

            Ok(Expr::AssignExpr {
                var: var_name,
                value: Box::new(value_expr),
                span,
            })
        }
        // Handle prefixed string literals (r"...", raw"...", big"...", etc.)
        _ if node.kind() == "prefixed_string_literal" => {
            // PrefixedStringLiteral has two children: [prefix, string]
            let children = walker.named_children(&node);
            if children.len() >= 2 {
                let prefix_text = walker.text(&children[0]);
                let string_text = walker.text(&children[1]);
                let content = string_text.trim_matches('"').to_string();

                match prefix_text {
                    "big" => {
                        // big"..." creates BigInt or BigFloat depending on content
                        if content.contains('.') || content.contains('e') || content.contains('E') {
                            Ok(Expr::Literal(Literal::BigFloat(content), span))
                        } else {
                            Ok(Expr::Literal(Literal::BigInt(content), span))
                        }
                    }
                    "raw" => {
                        // raw"..." creates a raw string literal
                        // In Julia, raw strings still process \\ (to \) and \" (to ")
                        // but all other escape sequences are kept as-is
                        let processed = process_raw_string_escapes(&content);
                        Ok(Expr::Literal(Literal::Str(processed), span))
                    }
                    "r" => {
                        // r"..." is a regex literal in Julia
                        // Create a Regex literal with the pattern
                        Ok(Expr::Literal(
                            Literal::Regex {
                                pattern: content,
                                flags: String::new(),
                            },
                            span,
                        ))
                    }
                    "MIME" => {
                        // MIME"text/plain" -> _mime_construct("text/plain")
                        // This creates a MIME{Symbol("text/plain")} type instance
                        Ok(Expr::Call {
                            function: "_mime_construct".to_string(),
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "v" => {
                        // v"1.2.3" creates a VersionNumber
                        // Parse version string and create constructor call
                        let parts: Vec<&str> = content.split('.').collect();
                        let major = parts
                            .first()
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let minor = parts
                            .get(1)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        let patch = parts
                            .get(2)
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(0);
                        Ok(Expr::Call {
                            function: "VersionNumber".to_string(),
                            args: vec![
                                Expr::Literal(Literal::Int(major), span),
                                Expr::Literal(Literal::Int(minor), span),
                                Expr::Literal(Literal::Int(patch), span),
                            ],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                    "b" => {
                        // b"data" creates a byte array (Vector{UInt8})
                        // Convert string to array of UInt8 values
                        let bytes: Vec<Expr> = content
                            .bytes()
                            .map(|b| Expr::Literal(Literal::Int(b as i64), span))
                            .collect();
                        let len = bytes.len();
                        Ok(Expr::ArrayLiteral {
                            elements: bytes,
                            shape: vec![len],
                            span,
                        })
                    }
                    "Int128" => {
                        // Int128"123" creates an Int128 literal
                        if let Ok(val) = content.parse::<i128>() {
                            Ok(Expr::Literal(Literal::Int128(val), span))
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid Int128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "UInt128" => {
                        // UInt128"123" creates a UInt128 literal
                        // Note: We store as i128 but the type system will treat it as UInt128
                        if let Ok(val) = content.parse::<u128>() {
                            // Convert to i128 for storage (bit pattern preservation)
                            Ok(Expr::Literal(Literal::Int128(val as i128), span))
                        } else {
                            Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(format!(
                                    "Invalid UInt128 literal: {}",
                                    content
                                )),
                                span,
                            ))
                        }
                    }
                    "s" => {
                        // s"..." creates a SubstitutionString (used in replace)
                        // For now, just return as regular string
                        Ok(Expr::Literal(Literal::Str(content), span))
                    }
                    _ => {
                        // Unknown prefixes are string macros in Julia:
                        // prefix"text" → @prefix_str("text")
                        // e.g., html"<b>bold</b>" → @html_str("<b>bold</b>")
                        //       L"x^2" → @L_str("x^2")
                        let macro_name = format!("{}_str", prefix_text);
                        Ok(Expr::Call {
                            function: macro_name,
                            args: vec![Expr::Literal(Literal::Str(content), span)],
                            kwargs: Vec::new(),
                            splat_mask: Vec::new(),
                            kwargs_splat_mask: Vec::new(),
                            span,
                        })
                    }
                }
            } else {
                let text = walker.text(&node);
                Ok(Expr::Literal(Literal::Str(text.to_string()), span))
            }
        }
        // Jump expressions: used in short-circuit context like `cond && return x`
        NodeKind::ReturnStatement => {
            let children = walker.named_children(&node);
            let value = if children.is_empty() {
                None
            } else {
                Some(Box::new(lower_expr_with_ctx(
                    walker,
                    children[0],
                    lambda_ctx,
                )?))
            };
            Ok(Expr::ReturnExpr { value, span })
        }
        NodeKind::BreakStatement => Ok(Expr::BreakExpr { span }),
        NodeKind::ContinueStatement => Ok(Expr::ContinueExpr { span }),
        // Handle typed expressions like Int64[] (typed empty array)
        NodeKind::TypedExpression => {
            let children = walker.named_children(&node);
            if children.len() == 2 {
                let type_node = children[0];
                let value_node = children[1];

                // Check if this is a typed empty array: Type[]
                if walker.kind(&type_node) == NodeKind::Identifier
                    && walker.kind(&value_node) == NodeKind::VectorExpression
                {
                    let type_children = walker.named_children(&value_node);
                    if type_children.is_empty() {
                        // Int64[] or similar: typed empty array
                        let element_type = walker.text(&type_node).to_string();
                        return Ok(Expr::TypedEmptyArray { element_type, span });
                    }
                }
            }
            // Fall through to error for unsupported typed expressions
            Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(format!(
                    "typed_expression: {}",
                    walker.text(&node)
                )),
                span,
            ))
        }
        // Bare operator as expression: treat as function reference (Issue #1985)
        NodeKind::Operator => {
            let op_text = walker.text(&node).to_string();
            Ok(Expr::FunctionRef {
                name: op_text,
                span,
            })
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(node.kind().to_string()),
            span,
        )),
    }
}

/// Lower a parametric type expression like `Complex{Float64}` or `Complex{promote_type(T, S)}`.
///
/// For static types (all type args are concrete type names), emits TypeOf builtin.
/// For dynamic types (type args contain function calls or type variables that aren't
/// known concrete types), emits DynamicTypeConstruct which evaluates type args at runtime.
fn lower_parametrized_type_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    // Extract base name (first identifier) and type arguments
    let mut base_name: Option<String> = None;
    let mut type_arg_nodes: Vec<Node<'a>> = Vec::new();

    for child in children {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if base_name.is_none() {
                    base_name = Some(walker.text(&child).to_string());
                } else {
                    // Type argument is an identifier
                    type_arg_nodes.push(child);
                }
            }
            NodeKind::CurlyExpression => {
                // Type arguments inside curly braces
                for type_arg in walker.named_children(&child) {
                    type_arg_nodes.push(type_arg);
                }
            }
            _ => {
                // Other children are type arguments
                type_arg_nodes.push(child);
            }
        }
    }

    // Check if any type argument is dynamic (call expression, or identifier that's not
    // a known concrete type)
    let has_dynamic_arg = type_arg_nodes
        .iter()
        .any(|n| is_dynamic_type_arg(walker, *n));

    if !has_dynamic_arg {
        // All static - use current behavior with TypeOf
        let name = walker.text(&node).to_string();
        Ok(Expr::Builtin {
            name: crate::ir::core::BuiltinOp::TypeOf,
            args: vec![Expr::Literal(crate::ir::core::Literal::Str(name), span)],
            span,
        })
    } else {
        // Has dynamic args - emit DynamicTypeConstruct
        let base = base_name.ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(
                    "parametric type without base name".to_string(),
                ),
                span,
            )
        })?;

        // Lower each type argument as an expression
        let type_args: Vec<Expr> = type_arg_nodes
            .iter()
            .map(|n| lower_expr(walker, *n))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Expr::DynamicTypeConstruct {
            base,
            type_args,
            span,
        })
    }
}

/// Check if a type argument node is dynamic (requires runtime evaluation).
/// Dynamic arguments include:
/// - Call expressions (e.g., promote_type(T, S))
///
/// Note: Simple type variable identifiers (like T in Rational{T}) are NOT considered
/// dynamic because the existing string-based TypeOf builtin handles them correctly
/// by substituting bound type parameters at runtime.
fn is_dynamic_type_arg<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    match walker.kind(&node) {
        NodeKind::CallExpression => true, // Function calls are always dynamic
        NodeKind::Identifier => {
            // Simple identifiers are NOT dynamic - the existing TypeOf builtin
            // handles type variable substitution via string-based lookup.
            // Only function calls require the DynamicTypeConstruct path.
            false
        }
        NodeKind::ParametrizedTypeExpression => {
            // Nested parametric type - recursively check
            let children = walker.named_children(&node);
            children
                .iter()
                .skip(1)
                .any(|c| is_dynamic_type_arg(walker, *c))
        }
        _ => false,
    }
}
