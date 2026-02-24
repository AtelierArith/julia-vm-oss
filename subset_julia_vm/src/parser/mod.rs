pub mod cst;
pub mod span;

// ============================================================================
// ParseOutcome - result of parsing
// ============================================================================

/// Outcome of parsing Julia source code.
pub enum ParseOutcome {
    /// Parsed using pure Rust parser
    Rust(RustParsedSource),
}

/// Wrapper for pure Rust parser output
pub struct RustParsedSource {
    pub cst: subset_julia_vm_parser::CstNode,
    pub source: String,
}

impl RustParsedSource {
    pub fn root(&self) -> &subset_julia_vm_parser::CstNode {
        &self.cst
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

// ============================================================================
// Parser - main parser interface
// ============================================================================

/// Parser using pure Rust backend
pub struct Parser;

impl Parser {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self)
    }

    pub fn parse(&mut self, source: &str) -> Result<ParseOutcome, crate::error::SyntaxError> {
        use subset_julia_vm_parser::Parser as RustParser;

        let parser = RustParser::new(source);
        let (cst, errors) = parser.parse();

        // Check for parse errors
        if !errors.is_empty() {
            let error_messages: Vec<String> = errors.iter().map(|e| format!("{}", e)).collect();
            return Err(crate::error::SyntaxError::parse_failed(
                error_messages.join("; "),
            ));
        }

        Ok(ParseOutcome::Rust(RustParsedSource {
            cst,
            source: source.to_string(),
        }))
    }
}

/// Parse and lower a single expression from text.
/// Used for string interpolation parsing.
pub fn parse_and_lower_expr(source: &str) -> Result<crate::ir::core::Expr, String> {
    use crate::lowering::expr::lower_expr;
    use crate::parser::cst::{CstWalker, Node};

    // Parse using pure Rust parser
    let parser = subset_julia_vm_parser::Parser::new(source);
    let (cst, errors) = parser.parse();

    if !errors.is_empty() {
        return Err(errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; "));
    }

    // Get the first expression from the source file
    let walker = CstWalker::new(source);

    // Get the first statement/expression from the source file
    let children: Vec<_> = cst.children.iter().collect();
    if let Some(first) = children.first() {
        let node = Node::new(first, source);
        lower_expr(&walker, node).map_err(|e| e.to_string())
    } else {
        Err("Empty expression".to_string())
    }
}
