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
}
