//! Concrete Syntax Tree (CST) node structure
//!
//! Provides a tree structure compatible with tree-sitter's output format.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::node_kind::NodeKind;
use crate::span::Span;

/// A node in the Concrete Syntax Tree
///
/// `text` was previously stored as an owned `Option<String>` on every leaf
/// node (identifiers, literals, operators — the bulk of all nodes a parse
/// produces). It is always recoverable from `span` via
/// `&source[span.byte_range()]`, and every production consumer already reads
/// text that way (`CstWalker::text` / `text_from_source`, and the `Meta.parse`
/// builtin's `cst_to_value`) rather than through the stored field, so the
/// field was pure allocation overhead with no runtime beneficiary. Call
/// [`CstNode::text_from_source`] with the original source string instead
/// (Issue #10126). `field_name` uses borrowed static field labels during parser
/// construction so fixed field names do not allocate (Issue #10188).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CstNode {
    /// The kind of node
    pub kind: NodeKind,

    /// Source location span
    pub span: Span,

    /// Whether this is a named node (vs anonymous punctuation)
    pub is_named: bool,

    /// Child nodes
    pub children: Vec<CstNode>,

    /// Field name if this node is a named field of its parent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<Cow<'static, str>>,
}

impl CstNode {
    /// Create a new CST node
    pub fn new(kind: NodeKind, span: Span) -> Self {
        Self {
            kind,
            span,
            is_named: kind.is_named(),
            children: Vec::new(),
            field_name: None,
        }
    }

    /// Create a new leaf node. Text is not stored — recover it from `span`
    /// via [`CstNode::text_from_source`] (Issue #10126).
    pub fn leaf(kind: NodeKind, span: Span) -> Self {
        Self {
            kind,
            span,
            is_named: kind.is_named(),
            children: Vec::new(),
            field_name: None,
        }
    }

    /// Create a new node with children
    pub fn with_children(kind: NodeKind, span: Span, children: Vec<CstNode>) -> Self {
        Self {
            kind,
            span,
            is_named: kind.is_named(),
            children,
            field_name: None,
        }
    }

    /// Add a child node
    pub fn push_child(&mut self, child: CstNode) {
        self.children.push(child);
    }

    /// Add a named field child
    pub fn push_field(&mut self, field_name: &'static str, child: CstNode) {
        let mut child = child;
        child.field_name = Some(Cow::Borrowed(field_name));
        self.children.push(child);
    }

    /// Get child by index
    pub fn child(&self, index: usize) -> Option<&CstNode> {
        self.children.get(index)
    }

    /// Get child by field name
    pub fn child_by_field(&self, name: &str) -> Option<&CstNode> {
        self.children
            .iter()
            .find(|c| c.field_name.as_deref() == Some(name))
    }

    /// Get all children by field name
    pub fn children_by_field<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a CstNode> {
        self.children
            .iter()
            .filter(move |c| c.field_name.as_deref() == Some(name))
    }

    /// Get named children (excluding anonymous punctuation) as iterator
    pub fn named_children(&self) -> impl Iterator<Item = &CstNode> {
        self.children.iter().filter(|c| c.is_named)
    }

    /// Get named children as Vec (tree-sitter compatible)
    ///
    /// This matches the signature used by CstWalker in subset_julia_vm
    pub fn named_children_vec(&self) -> Vec<&CstNode> {
        self.children.iter().filter(|c| c.is_named).collect()
    }

    /// Get all children as Vec (tree-sitter compatible)
    pub fn children_vec(&self) -> Vec<&CstNode> {
        self.children.iter().collect()
    }

    /// Find a child with the given node kind
    pub fn find_child(&self, kind: NodeKind) -> Option<&CstNode> {
        self.children.iter().find(|c| c.kind == kind)
    }

    /// Find all children with the given node kind
    pub fn find_children(&self, kind: NodeKind) -> impl Iterator<Item = &CstNode> {
        self.children.iter().filter(move |c| c.kind == kind)
    }

    /// Get the number of children
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// Get the number of named children
    pub fn named_child_count(&self) -> usize {
        self.children.iter().filter(|c| c.is_named).count()
    }

    /// Check if this node has errors
    pub fn has_error(&self) -> bool {
        self.kind == NodeKind::Error || self.children.iter().any(|c| c.has_error())
    }

    /// Get all error nodes
    pub fn errors(&self) -> Vec<&CstNode> {
        let mut errors = Vec::new();
        self.collect_errors(&mut errors);
        errors
    }

    fn collect_errors<'a>(&'a self, errors: &mut Vec<&'a CstNode>) {
        if self.kind == NodeKind::Error {
            errors.push(self);
        }
        for child in &self.children {
            child.collect_errors(errors);
        }
    }

    /// Get the text from source
    pub fn text_from_source<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span.start..self.span.end]
    }

    /// Walk the tree in pre-order
    pub fn walk(&self) -> CstWalker<'_> {
        CstWalker::new(self)
    }

    /// Convert to JSON-compatible format for WASM.
    ///
    /// `source` is the original source string the node was parsed from; leaf
    /// nodes (no children) include their `text` derived from `span`, matching
    /// the previous stored-`text`-field shape (Issue #10126).
    pub fn to_json(&self, source: &str) -> serde_json::Value {
        let text = if self.children.is_empty() {
            Some(self.text_from_source(source))
        } else {
            None
        };
        serde_json::json!({
            "type": self.kind.as_str(),
            "start": self.span.start,
            "end": self.span.end,
            "start_line": self.span.start_line,
            "end_line": self.span.end_line,
            "start_column": self.span.start_column,
            "end_column": self.span.end_column,
            "is_named": self.is_named,
            "text": text,
            "children": self.children.iter().map(|c| c.to_json(source)).collect::<Vec<_>>()
        })
    }

    /// Print AST structure in a human-readable format for debugging.
    ///
    /// This is useful for understanding the tree structure when writing tests
    /// or debugging parser issues. `source` is the original source string the
    /// node was parsed from, used to recover leaf text from `span` (Issue
    /// #10126).
    ///
    /// # Example output
    /// ```text
    /// Assignment
    ///   name: Identifier = "x"
    ///   value: IntegerLiteral = "42"
    /// ```
    pub fn debug_ast(&self, indent: usize, source: &str) {
        let pad = "  ".repeat(indent);

        // Build the line: [field_name: ]NodeKind[ = "text"]
        let field_prefix = match &self.field_name {
            Some(name) => format!("{}: ", name),
            None => String::new(),
        };

        let text_suffix = if self.children.is_empty() {
            format!(" = {:?}", self.text_from_source(source))
        } else {
            String::new()
        };

        println!("{}{}{:?}{}", pad, field_prefix, self.kind, text_suffix);

        for child in &self.children {
            child.debug_ast(indent + 1, source);
        }
    }

    /// Return AST structure as a string for debugging.
    ///
    /// Similar to `debug_ast()` but returns a String instead of printing.
    pub fn debug_ast_string(&self, source: &str) -> String {
        let mut output = String::new();
        self.debug_ast_to_string(&mut output, 0, source);
        output
    }

    fn debug_ast_to_string(&self, output: &mut String, indent: usize, source: &str) {
        use std::fmt::Write;

        let pad = "  ".repeat(indent);

        let field_prefix = match &self.field_name {
            Some(name) => format!("{}: ", name),
            None => String::new(),
        };

        let text_suffix = if self.children.is_empty() {
            format!(" = {:?}", self.text_from_source(source))
        } else {
            String::new()
        };

        // Writing to a `String` via `std::fmt::Write` cannot fail (its
        // `write_str` always returns `Ok(())`), so discard the result instead
        // of an infallible-but-panicking unwrap call that could otherwise
        // turn a future `output` type change into a live panic path
        // (Issue #10904).
        let _ = writeln!(
            output,
            "{}{}{:?}{}",
            pad, field_prefix, self.kind, text_suffix
        );

        for child in &self.children {
            child.debug_ast_to_string(output, indent + 1, source);
        }
    }

    /// Create from JSON-compatible format. The `text` field (if present) is
    /// ignored — it is no longer stored on `CstNode`; recover leaf text from
    /// `span` via [`CstNode::text_from_source`] against the original source
    /// instead (Issue #10126).
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        let kind = value.get("type")?.as_str()?.parse::<NodeKind>().ok()?;
        let span = Span::new(
            value.get("start")?.as_u64()? as usize,
            value.get("end")?.as_u64()? as usize,
            value.get("start_line")?.as_u64()? as usize,
            value.get("end_line")?.as_u64()? as usize,
            value.get("start_column")?.as_u64()? as usize,
            value.get("end_column")?.as_u64()? as usize,
        );
        let is_named = value.get("is_named")?.as_bool()?;
        let children = value
            .get("children")?
            .as_array()?
            .iter()
            .filter_map(CstNode::from_json)
            .collect();

        Some(Self {
            kind,
            span,
            is_named,
            children,
            field_name: None,
        })
    }
}

/// Tree walker for pre-order traversal
pub struct CstWalker<'a> {
    stack: Vec<&'a CstNode>,
}

impl<'a> CstWalker<'a> {
    fn new(root: &'a CstNode) -> Self {
        Self { stack: vec![root] }
    }
}

impl<'a> Iterator for CstWalker<'a> {
    type Item = &'a CstNode;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        // Push children in reverse order so we visit them left-to-right
        self.stack.extend(node.children.iter().rev());
        Some(node)
    }
}

/// Builder for constructing CST nodes
pub struct CstBuilder {
    source: String,
}

impl CstBuilder {
    /// Create a new builder with source code
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    /// Get the source code
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Extract text from a span
    pub fn text(&self, span: &Span) -> &str {
        &self.source[span.start..span.end]
    }
}

/// Test utilities for parser tests
///
/// Provides helper macros and functions for debugging parser test failures.
#[cfg(any(test, feature = "testing"))]
// Test-only assertion helpers legitimately dump AST diagnostics to stderr just
// before panicking, so the failure output reaches the test runner. This is the
// idiomatic test-helper pattern and is exempt from the Issue #2888 policy that
// targets production library code paths.
#[allow(clippy::print_stderr)]
pub mod testing {
    use super::*;

    /// Assert that a node has the expected kind, showing AST structure on failure.
    ///
    /// `source` is the original source string the node was parsed from (used
    /// only for the diagnostic dump on failure; Issue #10126).
    ///
    /// # Example
    /// ```ignore
    /// use subset_julia_vm_parser::cst::testing::assert_node_kind;
    /// let node = parse_expr("x + 1");
    /// assert_node_kind(&node, NodeKind::BinaryExpression, "x + 1");
    /// ```
    pub fn assert_node_kind(node: &CstNode, expected: NodeKind, source: &str) {
        if node.kind != expected {
            eprintln!("\n=== AST Structure (on assertion failure) ===\n");
            eprintln!("{}", node.debug_ast_string(source));
            panic!("Expected node kind {:?}, but got {:?}", expected, node.kind);
        }
    }

    /// Assert that a node has the expected text, showing AST structure on failure.
    pub fn assert_node_text(node: &CstNode, expected: &str, source: &str) {
        let actual = node.text_from_source(source);
        if actual != expected {
            eprintln!("\n=== AST Structure (on assertion failure) ===\n");
            eprintln!("{}", node.debug_ast_string(source));
            panic!("Expected node text {:?}, but got {:?}", expected, actual);
        }
    }

    /// Assert that a node has the expected child count, showing AST structure on failure.
    pub fn assert_child_count(node: &CstNode, expected: usize, source: &str) {
        let actual = node.children.len();
        if actual != expected {
            eprintln!("\n=== AST Structure (on assertion failure) ===\n");
            eprintln!("{}", node.debug_ast_string(source));
            panic!("Expected {} children, but got {}", expected, actual);
        }
    }

    /// Assert that a node has a child with the given field name and kind.
    pub fn assert_field_kind(node: &CstNode, field: &str, expected: NodeKind, source: &str) {
        match node.child_by_field(field) {
            Some(child) if child.kind == expected => {}
            Some(child) => {
                eprintln!("\n=== AST Structure (on assertion failure) ===\n");
                eprintln!("{}", node.debug_ast_string(source));
                panic!(
                    "Expected field '{}' to have kind {:?}, but got {:?}",
                    field, expected, child.kind
                );
            }
            None => {
                eprintln!("\n=== AST Structure (on assertion failure) ===\n");
                eprintln!("{}", node.debug_ast_string(source));
                panic!("Expected field '{}' to exist, but it was not found", field);
            }
        }
    }

    /// Dump AST structure for a parsed source (convenience function for tests).
    pub fn dump_parse_result(source: &str) {
        let parser = crate::Parser::new(source);
        let (cst, errors) = parser.parse();

        eprintln!("\n=== Source ===");
        eprintln!("{}", source);
        eprintln!("\n=== AST Structure ===\n");
        eprintln!("{}", cst.debug_ast_string(source));

        if !errors.is_empty() {
            eprintln!("=== Parse Errors ===");
            for error in &errors {
                eprintln!("  {}", error);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_create_node() {
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let node = CstNode::new(NodeKind::Identifier, span);

        assert_eq!(node.kind, NodeKind::Identifier);
        assert_eq!(node.span.start, 0);
        assert_eq!(node.span.end, 10);
        assert!(node.is_named);
        assert!(node.children.is_empty());
    }

    #[test]
    fn test_leaf_node() {
        let source = "12345";
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let node = CstNode::leaf(NodeKind::IntegerLiteral, span);

        assert_eq!(node.kind, NodeKind::IntegerLiteral);
        assert_eq!(node.text_from_source(source), "12345");
    }

    #[test]
    fn test_child_access() {
        let span = Span::new(0, 20, 1, 1, 1, 21);
        let child1 = CstNode::leaf(NodeKind::Identifier, Span::new(0, 3, 1, 1, 1, 4));
        let child2 = CstNode::leaf(NodeKind::IntegerLiteral, Span::new(6, 8, 1, 1, 7, 9));

        let mut parent = CstNode::new(NodeKind::Assignment, span);
        parent.push_field("name", child1);
        parent.push_field("value", child2);

        assert_eq!(parent.child_count(), 2);
        assert!(parent.child_by_field("name").is_some());
        assert!(parent.child_by_field("value").is_some());
        assert!(parent.child_by_field("other").is_none());
    }

    #[test]
    fn push_field_keeps_static_field_name_and_lookup_works() {
        use std::borrow::Cow;

        let span = Span::new(0, 20, 1, 1, 1, 21);
        let name = CstNode::leaf(NodeKind::Identifier, Span::new(0, 3, 1, 1, 1, 4));
        let value = CstNode::leaf(NodeKind::IntegerLiteral, Span::new(6, 8, 1, 1, 7, 9));

        let mut parent = CstNode::new(NodeKind::Assignment, span);
        parent.push_field("name", name);
        parent.push_field("value", value);

        assert!(matches!(
            parent.child(0).and_then(|child| child.field_name.as_ref()),
            Some(Cow::Borrowed("name"))
        ));
        assert_eq!(
            parent.child_by_field("name").map(|child| child.kind),
            Some(NodeKind::Identifier)
        );
        assert_eq!(
            parent
                .children_by_field("value")
                .map(|child| child.kind)
                .collect::<Vec<_>>(),
            vec![NodeKind::IntegerLiteral]
        );

        let serialized = serde_json::to_value(&parent).unwrap();
        let restored: CstNode = serde_json::from_value(serialized).unwrap();
        assert_eq!(
            restored.child_by_field("name").map(|child| child.kind),
            Some(NodeKind::Identifier)
        );
        assert!(matches!(
            restored
                .child(0)
                .and_then(|child| child.field_name.as_ref()),
            Some(Cow::Owned(name)) if name == "name"
        ));
    }

    #[test]
    fn test_walker() {
        let span = Span::new(0, 20, 1, 1, 1, 21);
        let child1 = CstNode::leaf(NodeKind::Identifier, Span::new(0, 3, 1, 1, 1, 4));
        let child2 = CstNode::leaf(NodeKind::IntegerLiteral, Span::new(6, 8, 1, 1, 7, 9));

        let parent = CstNode::with_children(NodeKind::Assignment, span, vec![child1, child2]);

        let kinds: Vec<_> = parent.walk().map(|n| n.kind).collect();
        assert_eq!(
            kinds,
            vec![
                NodeKind::Assignment,
                NodeKind::Identifier,
                NodeKind::IntegerLiteral
            ]
        );
    }

    #[test]
    fn test_json_roundtrip() {
        let source = "12345";
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let node = CstNode::leaf(NodeKind::IntegerLiteral, span);

        let json = node.to_json(source);
        let restored = CstNode::from_json(&json).unwrap();

        assert_eq!(restored.kind, node.kind);
        assert_eq!(restored.span, node.span);
        assert_eq!(
            restored.text_from_source(source),
            node.text_from_source(source)
        );
    }

    #[test]
    fn test_debug_ast_string() {
        let source = "x = 42";
        let span = Span::new(0, 20, 1, 1, 1, 21);
        let child1 = CstNode::leaf(NodeKind::Identifier, Span::new(0, 1, 1, 1, 1, 2));
        let child2 = CstNode::leaf(NodeKind::IntegerLiteral, Span::new(4, 6, 1, 1, 5, 7));

        let mut parent = CstNode::new(NodeKind::Assignment, span);
        parent.push_field("name", child1);
        parent.push_field("value", child2);

        let output = parent.debug_ast_string(source);

        // Should contain proper structure
        assert!(output.contains("Assignment"));
        assert!(output.contains("name: Identifier = \"x\""));
        assert!(output.contains("value: IntegerLiteral = \"42\""));

        // Should have proper indentation (children indented with 2 spaces)
        assert!(output.contains("  name:"));
        assert!(output.contains("  value:"));
    }

    #[test]
    fn test_debug_ast_nested() {
        // Create a nested structure: BinaryExpr -> Identifier, IntegerLiteral
        let source = "a + 1";
        let span = Span::new(0, 10, 1, 1, 1, 11);
        let left = CstNode::leaf(NodeKind::Identifier, Span::new(0, 1, 1, 1, 1, 2));
        let right = CstNode::leaf(NodeKind::IntegerLiteral, Span::new(4, 5, 1, 1, 5, 6));
        let op = CstNode::leaf(NodeKind::Operator, Span::new(2, 3, 1, 1, 3, 4));

        let mut binary = CstNode::new(NodeKind::BinaryExpression, span);
        binary.push_field("left", left);
        binary.push_child(op);
        binary.push_field("right", right);

        let output = binary.debug_ast_string(source);

        // Verify structure
        assert!(output.contains("BinaryExpression\n"));
        assert!(output.contains("  left: Identifier = \"a\""));
        assert!(output.contains("  Operator = \"+\""));
        assert!(output.contains("  right: IntegerLiteral = \"1\""));
    }
}
