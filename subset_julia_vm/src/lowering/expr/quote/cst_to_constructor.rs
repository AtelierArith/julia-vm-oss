//! CST to Expr constructor conversion for quote expressions.
//!
//! Contains `lower_quote_expr` (entry point) and `cst_to_expr_constructor`
//! which convert CST nodes to IR Expr constructors for quoted values.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::super::helpers::process_raw_string_escapes;
use super::super::literal::{parse_float, parse_int};

/// Lower a quote expression: :symbol or :(expr)
/// Converts to a QuoteLiteral that constructs the quoted value at runtime.
pub(crate) fn lower_quote_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let text = walker.text(&node);

    // Check if this is a simple symbol quote: :symbol
    if text.starts_with(':') && !text.starts_with(":(") {
        // Simple symbol quote: :foo -> Symbol("foo")
        let symbol_name = text.trim_start_matches(':');
        let constructor = Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args: vec![Expr::Literal(Literal::Str(symbol_name.to_string()), span)],
            span,
        };
        return Ok(Expr::QuoteLiteral {
            constructor: Box::new(constructor),
            span,
        });
    }

    // For complex quotes like :(expr), get the inner expression and convert to Expr constructor
    let children = walker.named_children(&node);
    if children.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("empty quote expression".to_string()),
            span,
        ));
    }

    // The first child is the quoted expression
    let inner_node = children[0];
    let constructor = cst_to_expr_constructor(walker, inner_node)?;

    Ok(Expr::QuoteLiteral {
        constructor: Box::new(constructor),
        span,
    })
}

/// Convert a CST node to an IR Expr that constructs the corresponding Expr/Symbol value at runtime.
/// This is used for quote expressions like :(1 + 2) which becomes Expr(:call, :+, 1, 2)
pub(in crate::lowering::expr) fn cst_to_expr_constructor<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);

    match walker.kind(&node) {
        // Literals become themselves (not quoted)
        NodeKind::IntegerLiteral => {
            let text = walker.text(&node);
            let value = parse_int(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match value {
                super::super::literal::ParsedInt::I64(v) => {
                    Ok(Expr::Literal(Literal::Int(v), span))
                }
                super::super::literal::ParsedInt::I128(v) => {
                    Ok(Expr::Literal(Literal::Int128(v), span))
                }
                super::super::literal::ParsedInt::BigInt(v) => {
                    Ok(Expr::Literal(Literal::BigInt(v), span))
                }
            }
        }
        NodeKind::FloatLiteral => {
            let text = walker.text(&node);
            let value = parse_float(text).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(text.to_string()),
                    span,
                )
            })?;
            match value {
                super::super::literal::ParsedFloat::F64(v) => {
                    Ok(Expr::Literal(Literal::Float(v), span))
                }
                super::super::literal::ParsedFloat::F32(v) => {
                    Ok(Expr::Literal(Literal::Float32(v), span))
                }
            }
        }
        NodeKind::StringLiteral => {
            let text = walker.text(&node);
            // Remove quotes
            let content = text.trim_matches('"').to_string();
            Ok(Expr::Literal(Literal::Str(content), span))
        }
        NodeKind::BooleanLiteral => {
            let text = walker.text(&node);
            let value = text == "true";
            Ok(Expr::Literal(Literal::Bool(value), span))
        }

        // Identifiers become Symbols: x -> :x
        NodeKind::Identifier => {
            let name = walker.text(&node);
            // Special cases for literals
            match name {
                "true" => Ok(Expr::Literal(Literal::Bool(true), span)),
                "false" => Ok(Expr::Literal(Literal::Bool(false), span)),
                "nothing" => Ok(Expr::Literal(Literal::Nothing, span)),
                "missing" => Ok(Expr::Literal(Literal::Missing, span)),
                _ => Ok(Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
                    span,
                }),
            }
        }

        // Binary expressions become Expr(:call, :op, left, right)
        NodeKind::BinaryExpression => {
            let children = walker.named_children(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "malformed binary expression".to_string(),
                    ),
                    span,
                ));
            }

            // Find operator - it's usually the middle element or has Operator kind
            let mut left_node = None;
            let mut op_text = None;
            let mut right_node = None;

            for child in &children {
                if walker.kind(child) == NodeKind::Operator {
                    op_text = Some(walker.text(child).to_string());
                } else if left_node.is_none() {
                    left_node = Some(*child);
                } else {
                    right_node = Some(*child);
                }
            }

            // If no Operator node found, try getting operator from text between children
            let op = op_text.unwrap_or_else(|| {
                // Fallback: extract operator from the middle
                if children.len() >= 2 {
                    let left_end = walker.span(&children[0]).end;
                    let right_start = walker.span(&children[children.len() - 1]).start;
                    let full_text = walker.text(&node);
                    let left_len = left_end - walker.span(&node).start;
                    let right_offset = right_start - walker.span(&node).start;
                    if right_offset > left_len
                        && right_offset <= full_text.len()
                        && left_len <= full_text.len()
                    {
                        full_text[left_len..right_offset].trim().to_string()
                    } else {
                        "+".to_string() // fallback
                    }
                } else {
                    "+".to_string()
                }
            });

            let left = left_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "missing left operand".to_string(),
                    ),
                    span,
                )
            })?;
            let right = right_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "missing right operand".to_string(),
                    ),
                    span,
                )
            })?;

            let left_constructor = cst_to_expr_constructor(walker, left)?;
            let right_constructor = cst_to_expr_constructor(walker, right)?;

            // Expr(:call, :op, left, right)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :call
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                        span,
                    },
                    // operator as symbol
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(op), span)],
                        span,
                    },
                    left_constructor,
                    right_constructor,
                ],
                span,
            })
        }

        // Call expressions become Expr(:call, :func, args...)
        NodeKind::CallExpression => {
            let children = walker.named_children(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty call expression".to_string(),
                    ),
                    span,
                ));
            }

            // First child is the function name
            let func_node = children[0];
            let func_name = walker.text(&func_node);

            let mut args = vec![
                // head: :call
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                    span,
                },
                // function name as symbol
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(func_name.to_string()), span)],
                    span,
                },
            ];

            // Remaining children are arguments
            for child in children.iter().skip(1) {
                // Check for ArgumentList
                if walker.kind(child) == NodeKind::ArgumentList {
                    for arg in walker.named_children(child) {
                        args.push(cst_to_expr_constructor(walker, arg)?);
                    }
                } else {
                    args.push(cst_to_expr_constructor(walker, *child)?);
                }
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Parenthesized expressions: unwrap the inner expression
        NodeKind::ParenthesizedExpression => {
            let children = walker.named_children(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "empty parenthesized expression".to_string(),
                    ),
                    span,
                ));
            }
            cst_to_expr_constructor(walker, children[0])
        }

        // Block: quote ... end -> Expr(:block, LineNumberNode, stmt1, LineNumberNode, stmt2, ...)
        // Julia inserts LineNumberNode before each statement in quote blocks
        NodeKind::Block | NodeKind::CompoundStatement => {
            let children = walker.named_children(&node);
            // Skip comments and get actual statements
            let stmts: Vec<_> = children
                .into_iter()
                .filter(|c| {
                    let k = walker.kind(c);
                    k != NodeKind::LineComment && k != NodeKind::BlockComment
                })
                .collect();

            if stmts.is_empty() {
                // Empty block returns nothing
                Ok(Expr::Literal(Literal::Nothing, span))
            } else {
                // Create Expr(:block, LineNumberNode, stmt1, LineNumberNode, stmt2, ...)
                let mut args = vec![Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                    span,
                }];
                for stmt in stmts {
                    // Insert LineNumberNode before each statement
                    let stmt_span = walker.span(&stmt);
                    args.push(Expr::Builtin {
                        name: BuiltinOp::LineNumberNodeNew,
                        args: vec![Expr::Literal(
                            Literal::Int(stmt_span.start_line as i64),
                            stmt_span,
                        )],
                        span: stmt_span,
                    });
                    args.push(cst_to_expr_constructor(walker, stmt)?);
                }
                Ok(Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args,
                    span,
                })
            }
        }

        // Assignment: x = expr -> Expr(:(=), :x, expr_constructor)
        NodeKind::Assignment => {
            let children = walker.named_children(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "assignment with insufficient children".to_string(),
                    ),
                    span,
                ));
            }

            let target = &children[0];
            let value = &children[children.len() - 1];

            // Get target name (should be an Identifier)
            let target_name = walker.text(target);
            let value_constructor = cst_to_expr_constructor(walker, *value)?;

            // Create Expr(:(=), :target, value)
            let args = vec![
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("=".to_string()), span)],
                    span,
                },
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(target_name.to_string()), span)],
                    span,
                },
                value_constructor,
            ];
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Local declaration: local x = expr -> Expr(:local, Expr(:(=), :x, expr))
        NodeKind::LocalStatement | NodeKind::LocalDeclaration => {
            let children = walker.named_children(&node);
            let mut inner_args = Vec::new();
            for child in &children {
                let child_kind = walker.kind(child);
                match child_kind {
                    NodeKind::Assignment | NodeKind::BinaryExpression => {
                        inner_args.push(cst_to_expr_constructor(walker, *child)?);
                    }
                    NodeKind::Identifier => {
                        let name = walker.text(child);
                        inner_args.push(Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
                            span,
                        });
                    }
                    _ => {}
                }
            }
            let mut args = vec![Expr::Builtin {
                name: BuiltinOp::SymbolNew,
                args: vec![Expr::Literal(Literal::Str("local".to_string()), span)],
                span,
            }];
            args.extend(inner_args);
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Unary expression with $ operator is interpolation - evaluate instead of quoting
        NodeKind::UnaryExpression => {
            let children = walker.named_children(&node);
            let text = walker.text(&node);

            // Check if this is $ interpolation
            if text.starts_with('$') && !children.is_empty() {
                // $ interpolation: $x or $(expr) -> evaluate and use as value
                let inner = &children[children.len() - 1];
                let inner_kind = walker.kind(inner);

                // Handle $(esc(expr)) - esc() marks expression as escaped from hygiene
                // We extract the inner expression and return it directly
                if inner_kind == NodeKind::ParenthesizedExpression {
                    let paren_children = walker.named_children(inner);
                    if !paren_children.is_empty() {
                        let paren_inner = paren_children[0];
                        if walker.kind(&paren_inner) == NodeKind::CallExpression {
                            let call_children = walker.named_children(&paren_inner);
                            if !call_children.is_empty() {
                                let func_name = walker.text(&call_children[0]);
                                if func_name == "esc" {
                                    // $(esc(expr)) - extract expr and return as Var
                                    // The argument to esc() is what we want
                                    if call_children.len() >= 2 {
                                        let esc_arg = &call_children[1];
                                        // Handle ArgumentList if present
                                        let actual_arg =
                                            if walker.kind(esc_arg) == NodeKind::ArgumentList {
                                                let arg_children = walker.named_children(esc_arg);
                                                if arg_children.is_empty() {
                                                    return Err(UnsupportedFeature::new(
                                                    UnsupportedFeatureKind::UnsupportedExpression(
                                                        "empty esc() call".to_string(),
                                                    ),
                                                    span,
                                                ));
                                                }
                                                arg_children[0]
                                            } else {
                                                *esc_arg
                                            };
                                        // Return the expression as a Var (it will be substituted during macro expansion)
                                        let arg_name = walker.text(&actual_arg);
                                        return Ok(Expr::Var(arg_name.to_string(), span));
                                    }
                                }
                            }
                        }
                        // Check for splat interpolation: $(p...)
                        if walker.kind(&paren_inner) == NodeKind::SplatExpression {
                            let splat_children = walker.named_children(&paren_inner);
                            if !splat_children.is_empty() {
                                let splat_inner = splat_children[0];
                                let inner_name = walker.text(&splat_inner);
                                // Return SplatInterpolation marker
                                return Ok(Expr::Builtin {
                                    name: BuiltinOp::SplatInterpolation,
                                    args: vec![Expr::Var(inner_name.to_string(), span)],
                                    span,
                                });
                            }
                        }
                        // Not esc() or splat, handle as regular parenthesized expression
                        // Fall through to get the variable name
                    }
                }

                // Simple interpolation: $x -> Var("x")
                let inner_name = walker.text(inner);
                Ok(Expr::Var(inner_name.to_string(), span))
            } else {
                // Other unary operators: -, !, ~, etc.
                if !children.is_empty() {
                    // Find operator
                    let op_text = text.chars().next().unwrap_or('+').to_string();
                    let operand = cst_to_expr_constructor(walker, children[children.len() - 1])?;

                    // Create Expr(:call, :op, operand)
                    let args = vec![
                        Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                            span,
                        },
                        Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(op_text), span)],
                            span,
                        },
                        operand,
                    ];
                    Ok(Expr::Builtin {
                        name: BuiltinOp::ExprNew,
                        args,
                        span,
                    })
                } else {
                    Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "empty unary expression".to_string(),
                        ),
                        span,
                    ))
                }
            }
        }

        // Macro calls inside quote blocks: @inner(x) -> Expr(:macrocall, Symbol("@inner"), nothing, x)
        NodeKind::MacroCall => {
            // Find the macro identifier
            let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier);
            let macro_name = match macro_ident {
                Some(ident) => walker.text(&ident).to_string(), // Keep the @ prefix
                None => {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(
                            "macro call without identifier".to_string(),
                        ),
                        span,
                    ))
                }
            };

            // Get arguments (all children except MacroIdentifier)
            let args: Vec<Node<'a>> = walker
                .named_children(&node)
                .into_iter()
                .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
                .collect();

            // Build Expr(:macrocall, Symbol("@inner"), nothing, args...)
            let mut expr_args = vec![
                // head: :macrocall
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("macrocall".to_string()), span)],
                    span,
                },
                // macro name as symbol (with @ prefix)
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(macro_name), span)],
                    span,
                },
                // LineNumberNode with line number from span
                Expr::Builtin {
                    name: BuiltinOp::LineNumberNodeNew,
                    args: vec![Expr::Literal(Literal::Int(span.start_line as i64), span)],
                    span,
                },
            ];

            // Add arguments
            for arg in args {
                expr_args.push(cst_to_expr_constructor(walker, arg)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: expr_args,
                span,
            })
        }

        // Tuple expression: (a, b, c) -> Expr(:tuple, a, b, c)
        // Named tuple: (a=1, b=2) -> Expr(:tuple, Expr(:(=), :a, 1), Expr(:(=), :b, 2))
        NodeKind::TupleExpression => {
            let children = walker.named_children(&node);

            let mut args = vec![
                // head: :tuple
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("tuple".to_string()), span)],
                    span,
                },
            ];

            // Add each element
            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Ternary expression: a ? b : c -> Expr(:if, a, b, c)
        NodeKind::TernaryExpression => {
            // Get operands (filter out ? and : operators)
            let operands: Vec<_> = walker
                .named_children(&node)
                .into_iter()
                .filter(|n| walker.kind(n) != NodeKind::Operator)
                .collect();

            if operands.len() != 3 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!(
                        "quote of ternary expression (expected 3 operands, got {})",
                        operands.len()
                    )),
                    span,
                ));
            }

            let condition = cst_to_expr_constructor(walker, operands[0])?;
            let then_expr = cst_to_expr_constructor(walker, operands[1])?;
            let else_expr = cst_to_expr_constructor(walker, operands[2])?;

            // Expr(:if, condition, then_expr, else_expr)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :if
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("if".to_string()), span)],
                        span,
                    },
                    condition,
                    then_expr,
                    else_expr,
                ],
                span,
            })
        }

        // If statement: if cond ... end -> Expr(:if, cond, then_block[, else_block])
        NodeKind::IfStatement => {
            let all_children: Vec<Node<'a>> = walker.children(&node);

            let mut condition: Option<Node<'a>> = None;
            let mut then_block: Option<Node<'a>> = None;
            let mut elseif_clauses: Vec<Node<'a>> = Vec::new();
            let mut else_block: Option<Node<'a>> = None;

            let mut i = 0;
            while i < all_children.len() {
                let child = all_children[i];
                let kind_str = child.kind();

                match kind_str {
                    "if" | "end" => {
                        // Skip keywords
                    }
                    "elseif_clause" => {
                        elseif_clauses.push(child);
                    }
                    "else" => {
                        // Next child should be the else block
                        i += 1;
                        if i < all_children.len() {
                            let else_node = all_children[i];
                            if walker.kind(&else_node) == NodeKind::Block {
                                else_block = Some(else_node);
                            }
                        }
                        break;
                    }
                    "else_clause" => {
                        // else_clause contains: else keyword + block
                        let else_all: Vec<Node<'a>> = walker.children(&child);
                        for else_child in else_all.iter() {
                            if else_child.kind() == "block" {
                                else_block = Some(*else_child);
                                break;
                            }
                        }
                    }
                    _ => {
                        // Must be condition or block
                        if condition.is_none() {
                            condition = Some(child);
                        } else if then_block.is_none() && walker.kind(&child) == NodeKind::Block {
                            then_block = Some(child);
                        }
                    }
                }
                i += 1;
            }

            let condition_node = condition.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of if expression: missing condition".to_string(),
                    ),
                    span,
                )
            })?;

            let then_block_node = then_block.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of if expression: missing then block".to_string(),
                    ),
                    span,
                )
            })?;

            let condition_expr = cst_to_expr_constructor(walker, condition_node)?;
            let then_expr = cst_to_expr_constructor(walker, then_block_node)?;

            let mut args = vec![
                // head: :if
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("if".to_string()), span)],
                    span,
                },
                condition_expr,
                then_expr,
            ];

            // Build the else/elseif chain
            // Process elseif clauses in reverse to build nested structure
            // if a; 1; elseif b; 2; else; 3; end
            // -> Expr(:if, :a, block1, Expr(:elseif, cond_block_b, block2, block3))
            if !elseif_clauses.is_empty() || else_block.is_some() {
                // Start from the else block (if any) and work backwards through elseifs
                let mut tail_expr: Option<Expr> = if let Some(else_node) = else_block {
                    Some(cst_to_expr_constructor(walker, else_node)?)
                } else {
                    None
                };

                // Process elseif clauses in reverse order
                for elseif_clause in elseif_clauses.into_iter().rev() {
                    let elseif_children = walker.named_children(&elseif_clause);
                    if elseif_children.len() < 2 {
                        return Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(
                                "quote of elseif: missing condition or body".to_string(),
                            ),
                            span,
                        ));
                    }

                    let elseif_cond = cst_to_expr_constructor(walker, elseif_children[0])?;
                    let elseif_body = cst_to_expr_constructor(walker, elseif_children[1])?;

                    // Wrap condition in a block for Julia AST compatibility
                    // Julia's elseif has condition wrapped: Expr(:elseif, Expr(:block, cond), body, else)
                    let cond_block = Expr::Builtin {
                        name: BuiltinOp::ExprNew,
                        args: vec![
                            Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                                span,
                            },
                            elseif_cond,
                        ],
                        span,
                    };

                    let mut elseif_args = vec![
                        Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str("elseif".to_string()), span)],
                            span,
                        },
                        cond_block,
                        elseif_body,
                    ];

                    if let Some(tail) = tail_expr {
                        elseif_args.push(tail);
                    }

                    tail_expr = Some(Expr::Builtin {
                        name: BuiltinOp::ExprNew,
                        args: elseif_args,
                        span,
                    });
                }

                if let Some(tail) = tail_expr {
                    args.push(tail);
                }
            } else if let Some(else_node) = else_block {
                // Simple if-else without elseif
                let else_expr = cst_to_expr_constructor(walker, else_node)?;
                args.push(else_expr);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // While statement: while cond ... end -> Expr(:while, cond, body)
        NodeKind::WhileStatement => {
            let children = walker.named_children(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of while: missing condition or body".to_string(),
                    ),
                    span,
                ));
            }

            let condition = cst_to_expr_constructor(walker, children[0])?;
            let body = cst_to_expr_constructor(walker, children[1])?;

            // Expr(:while, condition, body)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("while".to_string()), span)],
                        span,
                    },
                    condition,
                    body,
                ],
                span,
            })
        }

        // For statement: for i in iter ... end -> Expr(:for, :(i = iter), body)
        NodeKind::ForStatement => {
            let children = walker.named_children(&node);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of for: empty for statement".to_string(),
                    ),
                    span,
                ));
            }

            // Find ForBinding and Block
            let mut binding_node = None;
            let mut body_node = None;

            for child in &children {
                match walker.kind(child) {
                    NodeKind::ForBinding => {
                        if binding_node.is_some() {
                            // Multiple bindings not supported yet
                            return Err(UnsupportedFeature::new(
                                UnsupportedFeatureKind::UnsupportedExpression(
                                    "quote of for: multiple bindings not yet supported".to_string(),
                                ),
                                span,
                            ));
                        }
                        binding_node = Some(*child);
                    }
                    NodeKind::Block => {
                        body_node = Some(*child);
                    }
                    _ => {}
                }
            }

            let binding = binding_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of for: missing binding".to_string(),
                    ),
                    span,
                )
            })?;

            let body = body_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of for: missing body".to_string(),
                    ),
                    span,
                )
            })?;

            // Parse ForBinding: [var, iter] or [outer, var, iter]
            let binding_children = walker.named_children(&binding);
            let (var_node, iter_node) = if binding_children.len() >= 2 {
                // Check if first is "outer"
                let first_text = walker.text(&binding_children[0]);
                if first_text == "outer" && binding_children.len() >= 3 {
                    (binding_children[1], binding_children[2])
                } else {
                    (binding_children[0], binding_children[1])
                }
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of for: malformed binding".to_string(),
                    ),
                    span,
                ));
            };

            let var_expr = cst_to_expr_constructor(walker, var_node)?;
            let iter_expr = cst_to_expr_constructor(walker, iter_node)?;
            let body_expr = cst_to_expr_constructor(walker, body)?;

            // Create binding as Expr(:(=), var, iter)
            let binding_expr = Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("=".to_string()), span)],
                        span,
                    },
                    var_expr,
                    iter_expr,
                ],
                span,
            };

            // Expr(:for, binding, body)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("for".to_string()), span)],
                        span,
                    },
                    binding_expr,
                    body_expr,
                ],
                span,
            })
        }

        // Try statement: try ... catch e ... finally ... end
        // -> Expr(:try, try_block, catch_var_or_false, catch_block_or_false[, finally_block])
        NodeKind::TryStatement => {
            let mut try_block_node = None;
            let mut catch_clause_node = None;
            let mut finally_clause_node = None;

            for child in walker.named_children(&node) {
                match walker.kind(&child) {
                    NodeKind::Block if try_block_node.is_none() => {
                        try_block_node = Some(child);
                    }
                    NodeKind::CatchClause => catch_clause_node = Some(child),
                    NodeKind::FinallyClause => finally_clause_node = Some(child),
                    _ => {}
                }
            }

            // Convert try block
            let try_block_expr = match try_block_node {
                Some(block) => cst_to_expr_constructor(walker, block)?,
                None => Expr::Builtin {
                    name: BuiltinOp::ExprNew,
                    args: vec![Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                        span,
                    }],
                    span,
                },
            };

            // Parse catch clause
            let (catch_var_expr, catch_block_expr) = match catch_clause_node {
                Some(catch_node) => {
                    let mut var_name: Option<String> = None;
                    let mut block_node: Option<Node<'a>> = None;

                    for child in walker.named_children(&catch_node) {
                        match walker.kind(&child) {
                            NodeKind::Identifier if var_name.is_none() => {
                                var_name = Some(walker.text(&child).to_string());
                            }
                            NodeKind::Block if block_node.is_none() => {
                                block_node = Some(child);
                            }
                            _ => {}
                        }
                    }

                    let var_expr = match var_name {
                        Some(name) => Expr::Builtin {
                            name: BuiltinOp::SymbolNew,
                            args: vec![Expr::Literal(Literal::Str(name), span)],
                            span,
                        },
                        None => Expr::Literal(Literal::Bool(false), span),
                    };

                    let block_expr = match block_node {
                        Some(block) => cst_to_expr_constructor(walker, block)?,
                        None => Expr::Builtin {
                            name: BuiltinOp::ExprNew,
                            args: vec![Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                                span,
                            }],
                            span,
                        },
                    };

                    (var_expr, block_expr)
                }
                None => (
                    Expr::Literal(Literal::Bool(false), span),
                    Expr::Literal(Literal::Bool(false), span),
                ),
            };

            // Parse finally clause
            let finally_block_expr = match finally_clause_node {
                Some(finally_node) => {
                    let block_node = walker
                        .named_children(&finally_node)
                        .into_iter()
                        .find(|child| walker.kind(child) == NodeKind::Block);

                    match block_node {
                        Some(block) => Some(cst_to_expr_constructor(walker, block)?),
                        None => Some(Expr::Builtin {
                            name: BuiltinOp::ExprNew,
                            args: vec![Expr::Builtin {
                                name: BuiltinOp::SymbolNew,
                                args: vec![Expr::Literal(Literal::Str("block".to_string()), span)],
                                span,
                            }],
                            span,
                        }),
                    }
                }
                None => None,
            };

            // Build Expr(:try, try_block, catch_var, catch_block[, finally_block])
            let mut args = vec![
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("try".to_string()), span)],
                    span,
                },
                try_block_expr,
                catch_var_expr,
                catch_block_expr,
            ];

            if let Some(finally_expr) = finally_block_expr {
                args.push(finally_expr);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Arrow function: x -> expr -> Expr(:(->), params, body)
        NodeKind::ArrowFunctionExpression => {
            let children = walker.named_children(&node);

            // Filter out operator nodes
            let non_ops: Vec<_> = children
                .into_iter()
                .filter(|c| walker.kind(c) != NodeKind::Operator)
                .collect();

            if non_ops.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of arrow function: missing parameters or body".to_string(),
                    ),
                    span,
                ));
            }

            let params = cst_to_expr_constructor(walker, non_ops[0])?;
            let body = cst_to_expr_constructor(walker, non_ops[1])?;

            // Expr(:(->), params, body)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("->".to_string()), span)],
                        span,
                    },
                    params,
                    body,
                ],
                span,
            })
        }

        // Range expression: 1:10 -> Expr(:call, :(:), 1, 10)
        NodeKind::RangeExpression => {
            let children = walker.named_children(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of range: missing operands".to_string(),
                    ),
                    span,
                ));
            }

            // Range can have 2 or 3 operands (start:end or start:step:end)
            let mut args = vec![
                // head: :call
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                    span,
                },
                // function: :(:)
                Expr::Builtin {
                    name: BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(":".to_string()), span)],
                    span,
                },
            ];

            for child in children {
                args.push(cst_to_expr_constructor(walker, child)?);
            }

            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args,
                span,
            })
        }

        // Compound assignment: x -= 1 -> Expr(:-=, :x, 1)
        NodeKind::CompoundAssignment => {
            let children = walker.named_children(&node);

            // Find the operator and operands
            let mut left_node = None;
            let mut op_str = None;
            let mut right_node = None;

            for child in &children {
                match walker.kind(child) {
                    NodeKind::Operator => {
                        op_str = Some(walker.text(child).to_string());
                    }
                    _ => {
                        if left_node.is_none() {
                            left_node = Some(*child);
                        } else {
                            right_node = Some(*child);
                        }
                    }
                }
            }

            let left = left_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of compound assignment: missing left operand".to_string(),
                    ),
                    span,
                )
            })?;

            let op = op_str.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of compound assignment: missing operator".to_string(),
                    ),
                    span,
                )
            })?;

            let right = right_node.ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "quote of compound assignment: missing right operand".to_string(),
                    ),
                    span,
                )
            })?;

            let left_expr = cst_to_expr_constructor(walker, left)?;
            let right_expr = cst_to_expr_constructor(walker, right)?;

            // Expr(:-=, left, right) or Expr(:+=, left, right), etc.
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str(op), span)],
                        span,
                    },
                    left_expr,
                    right_expr,
                ],
                span,
            })
        }

        // Juxtaposition expression: 2x -> Expr(:call, :*, 2, x)
        NodeKind::JuxtapositionExpression => {
            let children = walker.named_children(&node);
            if children.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "juxtaposition expression missing operands".to_string(),
                    ),
                    span,
                ));
            }

            let left_constructor = cst_to_expr_constructor(walker, children[0])?;
            let right_constructor = cst_to_expr_constructor(walker, children[1])?;

            // Expr(:call, :*, left, right)
            Ok(Expr::Builtin {
                name: BuiltinOp::ExprNew,
                args: vec![
                    // head: :call
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("call".to_string()), span)],
                        span,
                    },
                    // operator: :*
                    Expr::Builtin {
                        name: BuiltinOp::SymbolNew,
                        args: vec![Expr::Literal(Literal::Str("*".to_string()), span)],
                        span,
                    },
                    left_constructor,
                    right_constructor,
                ],
                span,
            })
        }

        // Handle prefixed string literals in quote context (r"...", raw"...", big"...", etc.)
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
                        // For now, return an error as Regex is not yet implemented
                        Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedExpression(
                                "Regex literals (r\"...\") are not yet supported".to_string(),
                            ),
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
                        if let Ok(val) = content.parse::<u128>() {
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
                    _ => Ok(Expr::Literal(Literal::Str(content), span)),
                }
            } else {
                let text = walker.text(&node);
                Ok(Expr::Literal(Literal::Str(text.to_string()), span))
            }
        }

        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "quote for {} not yet supported",
                node.kind()
            )),
            span,
        )),
    }
}
