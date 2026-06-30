//! Tests migrated from tree-sitter-julia/test/corpus/expressions.txt

use subset_julia_vm_parser::{parse, NodeKind};

fn assert_parses(source: &str) {
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse: {}\nError: {:?}",
        source,
        result.err()
    );
}

fn assert_root_child_kind(source: &str, expected_kind: NodeKind) {
    let cst = parse(source).unwrap_or_else(|_| panic!("Failed to parse: {}", source));
    assert_eq!(cst.kind, NodeKind::SourceFile);
    assert!(!cst.children.is_empty(), "Expected at least one child");
    assert_eq!(
        cst.children[0].kind, expected_kind,
        "Expected {:?}, got {:?} for source: {}",
        expected_kind, cst.children[0].kind, source
    );
}

// =============================================================================
// Identifiers
// =============================================================================

#[test]
fn test_identifier_ascii() {
    assert_root_child_kind("foo", NodeKind::Identifier);
    assert_root_child_kind("foo_bar", NodeKind::Identifier);
    assert_root_child_kind("Foo123", NodeKind::Identifier);
}

#[test]
fn test_identifier_unicode() {
    assert_root_child_kind("α", NodeKind::Identifier);
    assert_root_child_kind("αβγ", NodeKind::Identifier);
    assert_root_child_kind("日本語", NodeKind::Identifier);
}

// Math symbols
#[test]
fn test_identifier_math_symbols() {
    assert_parses("∑");
    assert_parses("∫");
}

// Subscript identifiers
#[test]
fn test_identifier_subscript() {
    assert_parses("x₁");
    assert_parses("α₂");
}

// =============================================================================
// Field Expression
// =============================================================================

#[test]
fn test_field_expression() {
    assert_root_child_kind("a.b", NodeKind::FieldExpression);
    assert_root_child_kind("a.b.c", NodeKind::FieldExpression);
}

#[test]
fn test_field_expression_with_call() {
    assert_parses("a.b()");
    assert_parses("a.b.c()");
}

// =============================================================================
// Index Expression
// =============================================================================

#[test]
fn test_index_expression() {
    assert_root_child_kind("a[1]", NodeKind::IndexExpression);
    assert_root_child_kind("a[1, 2]", NodeKind::IndexExpression);
    assert_root_child_kind("a[]", NodeKind::IndexExpression);
    // a[1:end] - end keyword not yet supported
}

#[test]
fn test_index_expression_with_colon() {
    assert_root_child_kind("a[:]", NodeKind::IndexExpression);
    assert_root_child_kind("a[:, 1]", NodeKind::IndexExpression);
    assert_root_child_kind("a[1, :]", NodeKind::IndexExpression);
}

#[test]
fn test_index_expression_nested() {
    assert_root_child_kind("a[b[1]]", NodeKind::IndexExpression);
}

// =============================================================================
// Parametrized Expression
// =============================================================================

#[test]
fn test_parametrized_expression() {
    assert_root_child_kind("A{T}", NodeKind::ParametrizedTypeExpression);
    assert_root_child_kind("Dict{K, V}", NodeKind::ParametrizedTypeExpression);
    assert_root_child_kind("Array{T, N}", NodeKind::ParametrizedTypeExpression);
}

// Where clause - not yet implemented
// Parametrized with where
#[test]
fn test_parametrized_with_where() {
    assert_parses("A{T} where T");
    assert_parses("A{T} where T <: Number");
}

// =============================================================================
// Call Expression
// =============================================================================

#[test]
fn test_call_expression_no_args() {
    assert_root_child_kind("f()", NodeKind::CallExpression);
}

#[test]
fn test_call_expression_with_args() {
    assert_root_child_kind("f(x)", NodeKind::CallExpression);
    assert_root_child_kind("f(x, y)", NodeKind::CallExpression);
    assert_root_child_kind("f(x, y, z)", NodeKind::CallExpression);
}

#[test]
fn test_call_expression_trailing_comma() {
    assert_root_child_kind("f(x,)", NodeKind::CallExpression);
    assert_root_child_kind("f(x, y,)", NodeKind::CallExpression);
}

// Keyword args
#[test]
fn test_call_expression_keyword_args() {
    assert_parses("f(x; y=1)");
    assert_parses("f(x; a=1, b=2)");
    assert_parses("f(; x=1)"); // keyword args only
    assert_parses("f(a=1, b=2)"); // keyword args as positional (before semicolon)
}

// Splat in function call
#[test]
fn test_call_expression_splat() {
    assert_parses("f(x...)");
    assert_parses("f(args...; kwargs...)");
}

// Parametric call
#[test]
fn test_call_expression_parametric() {
    assert_parses("f{T}(x)");
    assert_parses("Array{Int}(undef, 10)");
}

// =============================================================================
// Broadcast Call Expression
// =============================================================================

#[test]
fn test_broadcast_call() {
    assert_root_child_kind("f.(x)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind("sin.(x)", NodeKind::BroadcastCallExpression);
}

// =============================================================================
// Do Block
// =============================================================================

#[test]
fn test_do_block() {
    assert_parses("map(xs) do x\n  x^2\nend");
    assert_parses("open(f) do io\n  read(io)\nend");
}

#[test]
fn test_do_block_with_args() {
    assert_parses("map(xs) do x, y\n  x + y\nend");
}

// =============================================================================
// Macro Call Expression
// =============================================================================

#[test]
fn test_macro_call_simple() {
    assert_root_child_kind("@time", NodeKind::MacrocallExpression);
    assert_root_child_kind("@show x", NodeKind::MacrocallExpression);
}

#[test]
fn test_macro_call_with_args() {
    assert_root_child_kind("@assert x > 0", NodeKind::MacrocallExpression);
    assert_root_child_kind("@test x == 1", NodeKind::MacrocallExpression);
}

#[test]
fn test_macro_call_with_parens() {
    assert_root_child_kind("@show(x)", NodeKind::MacrocallExpression);
    assert_root_child_kind("@assert(x > 0)", NodeKind::MacrocallExpression);
}

// Qualified macro calls - parsed differently
#[test]
fn test_macro_call_qualified() {
    assert_parses("Base.@time");
    assert_parses("Test.@test x == 1");
    // Module.@macro style
    assert_parses("Meta.@dump x");
    assert_parses("LinearAlgebra.@show A");
}

// Closed macro calls (with brackets or parens directly after @name)
#[test]
fn test_macro_call_closed() {
    assert_parses("@m[1, 2]");
    assert_parses("@m(x, y)");
    assert_parses("@enum(Color, red, green, blue)");
}

// Broadcast macro @.
#[test]
fn test_macro_broadcast() {
    assert_parses("@. x + y");
    assert_parses("@. a * x + b");
}

// Braces-argument macro calls (e.g. @NamedTuple{a::Int, b}) (Issue #5120).
#[test]
fn test_macro_call_braces() {
    assert_root_child_kind(
        "@NamedTuple{a::Int, b::String}",
        NodeKind::MacrocallExpression,
    );
    assert_root_child_kind("@NamedTuple{a, b}", NodeKind::MacrocallExpression);
    assert_root_child_kind("@NamedTuple{}", NodeKind::MacrocallExpression);
    assert_parses("@NamedTuple{x::Float64}");
    assert_parses("println(@NamedTuple{a::Int, b::String})");
}

// The braces argument is parsed into a CurlyExpression carrying the field decls.
#[test]
fn test_macro_call_braces_structure() {
    let cst = parse("@NamedTuple{a::Int, b::String}").expect("parse braces macro");
    let macrocall = &cst.children[0];
    assert_eq!(macrocall.kind, NodeKind::MacrocallExpression);
    // children: [MacroIdentifier, CurlyExpression]
    let braces = macrocall
        .children
        .iter()
        .find(|c| c.kind == NodeKind::CurlyExpression)
        .expect("braces argument should be a CurlyExpression");
    // Two field declarations.
    assert_eq!(braces.children.len(), 2, "expected two field decls");
}

// Space-separated macro arguments are whitespace-sensitive at the `(`/`[`
// boundary, just like upstream Julia (Issue #5494):
//   `@m foo (bar)` -> two arguments (`foo`, `bar`)
//   `@m foo(bar)`  -> one call argument (`foo(bar)`)
// Returns the argument kinds (excluding the leading MacroIdentifier).
fn macro_argument_kinds(source: &str) -> Vec<NodeKind> {
    let cst = parse(source).unwrap_or_else(|e| panic!("Failed to parse {source:?}: {e:?}"));
    let macrocall = &cst.children[0];
    assert_eq!(
        macrocall.kind,
        NodeKind::MacrocallExpression,
        "expected a macrocall for {source:?}"
    );
    macrocall
        .children
        .iter()
        .skip(1) // skip MacroIdentifier
        .map(|c| c.kind)
        .collect()
}

#[test]
fn test_macro_arg_space_before_paren_is_separate_arg() {
    // `@m Ident (expr)`: the space before `(` separates arguments, so this is
    // two macro arguments, NOT one call `Ident(expr)`.
    let kinds = macro_argument_kinds("@m Ident (expr)");
    assert_eq!(
        kinds,
        vec![NodeKind::Identifier, NodeKind::ParenthesizedExpression],
        "`@m Ident (expr)` must parse as two arguments"
    );
}

#[test]
fn test_macro_arg_no_space_before_paren_is_call() {
    // `@m foo(bar)`: no space, so `foo(bar)` is a single call argument.
    let kinds = macro_argument_kinds("@m foo(bar)");
    assert_eq!(
        kinds,
        vec![NodeKind::CallExpression],
        "`@m foo(bar)` must parse as one call argument"
    );
}

#[test]
fn test_macro_arg_test_throws_typed_paren() {
    // Regression for Issue #5494: `@test_throws TypeError (1 + 1)::Float64`
    // must be two arguments (`TypeError` and `(1 + 1)::Float64`), not one
    // argument `(TypeError(1 + 1))::Float64`.
    let kinds = macro_argument_kinds("@test_throws TypeError (1 + 1)::Float64");
    assert_eq!(
        kinds,
        vec![NodeKind::Identifier, NodeKind::TypedExpression],
        "`@test_throws TypeError (1 + 1)::Float64` must parse as two arguments"
    );
}

#[test]
fn test_macro_arg_space_sensitivity_more_cases() {
    // `@m foo[1] (bar)`: index fuses (adjacent), then `(bar)` is a new arg.
    assert_eq!(
        macro_argument_kinds("@m foo[1] (bar)"),
        vec![NodeKind::IndexExpression, NodeKind::ParenthesizedExpression]
    );
    // `@m foo(bar) (baz)`: call fuses (adjacent), then `(baz)` is a new arg.
    assert_eq!(
        macro_argument_kinds("@m foo(bar) (baz)"),
        vec![NodeKind::CallExpression, NodeKind::ParenthesizedExpression]
    );
    // `@m foo (bar)[1]`: `foo` is its own arg, `(bar)[1]` is the next arg.
    assert_eq!(
        macro_argument_kinds("@m foo (bar)[1]"),
        vec![NodeKind::Identifier, NodeKind::IndexExpression]
    );
    // `@m foo (bar) baz`: three arguments.
    assert_eq!(
        macro_argument_kinds("@m foo (bar) baz"),
        vec![
            NodeKind::Identifier,
            NodeKind::ParenthesizedExpression,
            NodeKind::Identifier
        ]
    );
}

// Guard: macro-argument whitespace sensitivity must NOT regress sjulia's
// lenient `f (x)` call parsing at ordinary expression position, nor inside a
// grouping that is itself a macro argument (Issue #5494).
#[test]
fn test_macro_arg_space_sensitivity_does_not_leak_into_groupings() {
    // `f (x)` at expression position still parses as a call in sjulia.
    let cst = parse("y = f (x)").expect("parse `y = f (x)`");
    let assignment = &cst.children[0];
    let call = assignment
        .children
        .iter()
        .find(|c| c.kind == NodeKind::CallExpression)
        .expect("`f (x)` should still be a call at expression position");
    assert_eq!(call.kind, NodeKind::CallExpression);

    // Inside an adjacent call argument, `g (x)` is still leniently a call.
    let kinds = macro_argument_kinds("@m f(g (x))");
    assert_eq!(
        kinds,
        vec![NodeKind::CallExpression],
        "`@m f(g (x))` should be one call argument; the interior stays lenient"
    );
}

// =============================================================================
// Quote Expression
// =============================================================================

#[test]
fn test_quote_expression() {
    assert_parses(":x");
    assert_parses(":(a + b)");
    assert_parses(":(f(x))");
}

#[test]
fn test_quote_block() {
    assert_parses("quote\n  x + 1\nend");
}

// =============================================================================
// Interpolation Expression
// =============================================================================

// Interpolation in quote
#[test]
fn test_interpolation_in_quote() {
    assert_parses(":($x)");
    assert_parses(":(a + $b)");
}

// =============================================================================
// Adjoint Expression
// =============================================================================

#[test]
fn test_adjoint_expression() {
    assert_root_child_kind("A'", NodeKind::AdjointExpression);
    assert_parses("(A * B)'");
}

// =============================================================================
// Juxtaposition Expression
// =============================================================================

#[test]
fn test_juxtaposition() {
    assert_parses("2x");
    assert_parses("2π");
    assert_parses("3im");
}

#[test]
fn test_juxtaposition_with_parens() {
    assert_parses("2(x + 1)");
    assert_parses("3(a * b)");
}

// =============================================================================
// Arrow Function Expression
// =============================================================================

#[test]
fn test_arrow_function_simple() {
    assert_root_child_kind("x -> x^2", NodeKind::ArrowFunctionExpression);
}

#[test]
fn test_arrow_function_multiple_args() {
    assert_root_child_kind("(x, y) -> x + y", NodeKind::ArrowFunctionExpression);
}

#[test]
fn test_arrow_function_pair_body_precedence() {
    let cst = parse("(x, y) -> x => y").expect("parse arrow function with pair body");
    let arrow = &cst.children[0];
    assert_eq!(arrow.kind, NodeKind::ArrowFunctionExpression);
    assert_eq!(arrow.children[1].kind, NodeKind::BinaryExpression);
    assert_eq!(arrow.children[1].children[1].text.as_deref(), Some("=>"));
}

#[test]
fn test_arrow_function_typed() {
    assert_parses("(x::Int) -> x * 2");
}

// Arrow with begin block
#[test]
fn test_arrow_function_block() {
    assert_parses("x -> begin\n  y = x + 1\n  y * 2\nend");
}

// =============================================================================
// Range Expression
// =============================================================================

#[test]
fn test_range_expression() {
    assert_root_child_kind("1:10", NodeKind::RangeExpression);
    assert_root_child_kind("1:2:10", NodeKind::RangeExpression);
}

// Range with end keyword - not yet implemented
#[test]
fn test_range_with_end() {
    assert_parses("1:end");
    assert_parses("a[1:end]");
    assert_parses("a[begin:end]");
}

// =============================================================================
// Typed Expression
// =============================================================================

#[test]
fn test_typed_expression() {
    assert_root_child_kind("x::Int", NodeKind::TypedExpression);
    // Parametric types x::Vector{Float64} - not yet supported
}

// =============================================================================
// Ternary Expression
// =============================================================================

#[test]
fn test_ternary_expression() {
    assert_root_child_kind("a ? b : c", NodeKind::TernaryExpression);
    assert_root_child_kind("x > 0 ? x : -x", NodeKind::TernaryExpression);
}

// =============================================================================
// Tuple Expression
// =============================================================================

#[test]
fn test_tuple_expression() {
    assert_root_child_kind("(1, 2)", NodeKind::TupleExpression);
    assert_root_child_kind("(1, 2, 3)", NodeKind::TupleExpression);
}

#[test]
fn test_tuple_empty() {
    assert_root_child_kind("()", NodeKind::TupleExpression);
}

#[test]
fn test_tuple_trailing_comma() {
    assert_root_child_kind("(1,)", NodeKind::TupleExpression);
}

// =============================================================================
// Parenthesized Expression
// =============================================================================

#[test]
fn test_parenthesized_expression() {
    assert_root_child_kind("(x)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(1 + 2)", NodeKind::ParenthesizedExpression);
}

// =============================================================================
// Splat Expression
// =============================================================================

#[test]
fn test_splat_expression() {
    assert_root_child_kind("x...", NodeKind::SplatExpression);
    assert_root_child_kind("args...", NodeKind::SplatExpression);
}

// =============================================================================
// Pair Expression
// =============================================================================

#[test]
fn test_pair_expression() {
    assert_parses("a => b");
    assert_parses(":key => value");

    let cst = parse(":f => +").expect("parse Pair with bare operator RHS");
    assert_eq!(cst.children.len(), 1);
    let pair = &cst.children[0];
    assert_eq!(pair.kind, NodeKind::BinaryExpression);
    assert_eq!(pair.children.len(), 3);
    assert_eq!(pair.children[0].kind, NodeKind::QuoteExpression);
    assert_eq!(pair.children[1].kind, NodeKind::Operator);
    assert_eq!(pair.children[1].text.as_deref(), Some("=>"));
    assert_eq!(pair.children[2].kind, NodeKind::Operator);
    assert_eq!(pair.children[2].text.as_deref(), Some("+"));
}

// =============================================================================
// Where Expression
// =============================================================================

#[test]
fn test_where_expression_simple() {
    assert_root_child_kind("T where T", NodeKind::WhereExpression);
    assert_root_child_kind("Array{T} where T", NodeKind::WhereExpression);
}

#[test]
fn test_where_expression_bounded() {
    assert_parses("T where T <: Number");
    assert_parses("Array{T} where T <: AbstractFloat");
    assert_parses("T where T >: Int");
}

#[test]
fn test_where_expression_multiple() {
    // Multiple where clauses (chained)
    assert_parses("Dict{K, V} where K where V");
    assert_parses("Array{T, N} where T where N");
}

#[test]
fn test_where_expression_in_function() {
    assert_parses("f(x::T) where T = x");
    assert_parses("function foo(x::T) where T; x; end");
}

#[test]
fn test_where_expression_complex() {
    assert_parses("Vector{T} where T <: Union{Int, Float64}");
    // Note: `where {T, S}` syntax is not yet supported
    // assert_parses("Tuple{T, S} where {T <: Number, S <: Number}");
    // Use chained where instead:
    assert_parses("Tuple{T, S} where S <: Number where T <: Number");
}
