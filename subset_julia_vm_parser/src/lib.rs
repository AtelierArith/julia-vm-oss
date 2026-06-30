// Prevent accidental debug output in library code (Issue #2888).
// CLI binaries (bin/) may use eprintln!() for user-facing error messages.
#![deny(clippy::print_stderr)]

//! subset_julia_vm_parser
//!
//! Pure Rust parser for Julia subset - faithful reimplementation of tree-sitter-julia.
//!
//! This crate provides a WASM-compatible parser that produces the same CST structure
//! as tree-sitter-julia, enabling unified parsing across Native and WASM targets.
//!
//! # Example
//!
//! ```
//! use subset_julia_vm_parser::{parse, NodeKind};
//!
//! let source = "42";
//! let cst = parse(source).expect("parse failed");
//!
//! assert_eq!(cst.kind, NodeKind::SourceFile);
//! ```

pub mod cst;
pub mod error;
pub mod lexer;
pub mod node_kind;
pub mod parser;
pub mod span;
pub mod token;

// Re-exports
pub use cst::{CstBuilder, CstNode, CstWalker};
pub use error::{ParseError, ParseErrors, ParseResult};
pub use lexer::{Lexer, SpannedToken};
pub use node_kind::NodeKind;
pub use parser::Parser;
pub use span::{SourceMap, Span};
pub use token::{Associativity, Precedence, Token};

// Test utilities (available in tests or with "testing" feature)
#[cfg(any(test, feature = "testing"))]
pub use cst::testing;

/// Parse Julia source code into a CST
///
/// Returns a `SourceFile` node containing the parsed program.
///
/// # Example
///
/// ```
/// use subset_julia_vm_parser::{parse, NodeKind};
///
/// let cst = parse("42").unwrap();
/// assert_eq!(cst.kind, NodeKind::SourceFile);
/// ```
pub fn parse(source: &str) -> ParseResult<CstNode> {
    let (cst, errors) = parser::parse(source);
    if errors.is_empty() {
        Ok(cst)
    } else {
        Err(errors.into_iter().next().unwrap())
    }
}

/// Parse Julia source code with error recovery
///
/// Returns a CST even if there are parse errors, along with a list of errors.
pub fn parse_with_errors(source: &str) -> (CstNode, ParseErrors) {
    parser::parse(source)
}

/// Tokenize Julia source code
///
/// Returns a vector of tokens with their spans.
pub fn tokenize(source: &str) -> Vec<Result<SpannedToken<'_>, ParseError>> {
    lexer::tokenize(source)
}

/// Get version information
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let cst = parse("").unwrap();
        assert_eq!(cst.kind, NodeKind::SourceFile);
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("1 + 2");
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn test_version() {
        assert!(!version().is_empty());
    }

    #[test]
    fn superscript_identifier_suffix_after_infix_parses_issue_8298() {
        let src = "dderiv⁻¹ = 2\nprintln(3 * dderiv⁻¹)\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected superscript identifier suffix after infix to parse, got {result:?}"
        );
    }

    #[test]
    fn multiline_return_tuple_after_comma_parses_issue_8304() {
        let src = "function f()\n    return 1,\n           2,\n           3\nend\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected multiline return tuple after comma to parse, got {result:?}"
        );
    }

    #[test]
    fn numeric_parenthesized_juxtaposition_parses_issue_8140() {
        let src = "i = 2\nm = 3\nprintln(1 + 2(i - 1) * m)\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected numeric parenthesized juxtaposition to parse, got {result:?}"
        );
    }

    #[test]
    fn macrocall_argument_inside_call_parses_before_comma_issue_7494() {
        let cst = parse("println(joinpath(@__DIR__, \"..\", \"animals.txt\"))").unwrap();
        let println_call = &cst.children[0];
        assert_eq!(println_call.kind, NodeKind::CallExpression);
        let println_args = &println_call.children[1];
        assert_eq!(println_args.kind, NodeKind::ArgumentList);
        let joinpath_call = &println_args.children[0];
        assert_eq!(joinpath_call.kind, NodeKind::CallExpression);
        let joinpath_args = &joinpath_call.children[1];
        assert_eq!(joinpath_args.kind, NodeKind::ArgumentList);
        assert_eq!(
            joinpath_args.children[0].kind,
            NodeKind::MacrocallExpression
        );
        assert_eq!(joinpath_args.children[1].kind, NodeKind::StringLiteral);
        assert_eq!(joinpath_args.children[2].kind, NodeKind::StringLiteral);
    }

    #[test]
    fn nested_quote_interpolation_call_arg_parses_issue_7507() {
        let src = r#"struct TypeBind
    name::Symbol
    ts::Set{Any}
end
name = :x
ts = [:call]
ex = Expr(:$, :($TypeBind($(Expr(:quote, name)), Set{Any}([$(ts...)]))))
"#;
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected nested quote interpolation in call arguments to parse, got {result:?}"
        );
    }

    #[test]
    fn quoted_semicolon_block_parses_issue_7511() {
        let src = r#"line = nothing
yes = :(1)
ex = :($line;$yes)
"#;
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected quoted semicolon block to parse, got {result:?}"
        );
    }

    #[test]
    fn prime_suffix_identifier_in_ternary_parses_issue_7513() {
        let src = "s = 1\ns′ = 2\nx = 1\nresult = x == s ? s′ : x\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected prime suffix identifier in ternary to parse, got {result:?}"
        );
    }

    #[test]
    fn comparison_in_ternary_then_branch_parses_issue_8314() {
        // The then-branch of a ternary may contain an un-parenthesized comparison
        // operator; the `:` after it is the ternary separator, not a range.
        for src in [
            "y = true ? 1 > 0 : 2 > 0\n",
            "y = true ? 1 == 0 : 2 == 0\n",
            "y = true ? 2 > 1 ? 10 : 20 : 30\n", // nested ternary in then-branch
        ] {
            let result = parse(src);
            assert!(
                result.is_ok(),
                "expected comparison in ternary then-branch to parse: {src:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn parenthesized_range_in_ternary_then_branch_still_parses_issue_8314() {
        // A genuine range inside a grouping in the then-branch must still parse:
        // entering `(...)` / `[...]` / call args clears the ternary-then context.
        for src in [
            "y = true ? (1 : 2) : 5\n",
            "y = true ? f(1 : 2) : 5\n",
            "y = true ? a[1 : 2] : 5\n",
        ] {
            let result = parse(src);
            assert!(
                result.is_ok(),
                "expected parenthesized range in ternary then-branch to parse: {src:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn range_in_ternary_else_branch_parses_as_range_issue_8318() {
        // A `:` range in the else-branch must stay a range: `cond ? a : b:c` is
        // `cond ? a : (b:c)`, not `(cond ? a : b):c`. The then-branch separator
        // detection must not leak into the else-branch.
        let cst = parse("y = true ? 1 : 4:6\n").unwrap();
        let assignment = &cst.children[0];
        assert_eq!(assignment.kind, NodeKind::Assignment);
        let ternary = &assignment.children[2];
        assert_eq!(
            ternary.kind,
            NodeKind::TernaryExpression,
            "expected the whole RHS to be a ternary (else = `4:6`), got {ternary:?}"
        );
        // else-branch is the range `4:6`
        let else_branch = &ternary.children[2];
        assert_eq!(else_branch.kind, NodeKind::RangeExpression);
    }

    #[test]
    fn quoted_single_element_tuple_interpolation_parses_issue_7514() {
        let cst = parse("arg = :x\nex = :($arg,)\n").unwrap();
        let assignment = &cst.children[1];
        assert_eq!(assignment.kind, NodeKind::Assignment);
        let quote = &assignment.children[2];
        assert_eq!(quote.kind, NodeKind::QuoteExpression);
        assert_eq!(quote.children[0].kind, NodeKind::TupleExpression);
        assert_eq!(quote.children[0].children.len(), 1);
    }

    #[test]
    fn pair_after_function_expression_parses_issue_7517() {
        let src = "ex = :(begin function f_(args__) body_ end => rhs end)\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected pair after block-form function expression to parse, got {result:?}"
        );
    }

    #[test]
    fn anonymous_function_expression_parses_issue_7518() {
        let src = "f = function (x) x + 1 end\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected anonymous function expression to parse, got {result:?}"
        );
    }

    #[test]
    fn parenthesized_operator_function_head_parses_issue_7519() {
        let src = "ex = :(function (fcall_ | fcall_) body_ end)\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected parenthesized operator function head to parse, got {result:?}"
        );
    }

    #[test]
    fn quoted_interpolated_function_name_parses_issue_7520() {
        let src = "fname = :f\nex = :(function $fname(x) x end)\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected interpolated function name in quote to parse, got {result:?}"
        );
    }

    #[test]
    fn quoted_function_parameter_interpolation_parses_issue_7522() {
        let src = "args = [:x]\nex = :(function f($(args...)) x end)\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected interpolated parameters in quoted function to parse, got {result:?}"
        );
    }

    #[test]
    fn quoted_interpolated_field_assignment_parses_issue_7523() {
        let src = "x = :obj\nf = :field\nv = :val\nex = :($x.$f += $v)\n";
        let result = parse(src);
        assert!(
            result.is_ok(),
            "expected quoted field expression with interpolated field to parse, got {result:?}"
        );
    }

    #[test]
    fn whitespace_macro_comma_args_parse_as_tuple_issue_7526() {
        let cst = parse("macro m(x); nothing; end\n@m a, b\n").unwrap();
        let macrocall = &cst.children[1];
        assert_eq!(macrocall.kind, NodeKind::MacrocallExpression);
        assert_eq!(macrocall.children.len(), 2);
        assert_eq!(macrocall.children[1].kind, NodeKind::TupleExpression);
        assert_eq!(macrocall.children[1].children.len(), 2);
    }
}
