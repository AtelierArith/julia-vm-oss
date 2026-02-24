//! CST (Concrete Syntax Tree) types and walker
//!
//! This module provides a unified interface for working with parsed Julia code,
//! abstracting over the underlying parser implementation (tree-sitter or pure Rust).

use crate::parser::span::Span;

// ============================================================================
// NodeKind enum - shared between all parser implementations
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    SourceFile,
    FunctionDefinition,
    ShortFunctionDefinition,
    Assignment,
    CompoundAssignment,
    ReturnStatement,
    ForStatement,
    WhileStatement,
    IfStatement,
    TryStatement,
    CatchClause,
    ElseClause,
    FinallyClause,
    BreakStatement,
    ContinueStatement,
    Block,
    BinaryExpression,
    UnaryExpression,
    CallExpression,
    RangeExpression,
    TupleExpression,
    ArgumentList,
    ParenthesizedExpression,
    ForBinding,
    Identifier,
    IntegerLiteral,
    FloatLiteral,
    StringLiteral,
    CharacterLiteral,
    StringInterpolation,
    StringContent,
    MacroCall,
    MacroIdentifier,
    MacroArgumentList,
    QuoteStatement,
    QuoteExpression,
    UsingStatement,
    ImportStatement,
    ExportStatement,
    PublicStatement,
    ModuleDefinition,
    BaremoduleDefinition,
    MacroDefinition,
    Operator,
    VectorExpression,
    MatrixExpression,
    MatrixRow,
    IndexExpression,
    ComprehensionExpression,
    GeneratorExpression,
    ForClause,
    IfClause,
    TypedParameter,
    Parameter,
    KwParameter,
    ParameterList,
    TypeClause,
    TypedExpression,
    Signature,
    StructDefinition,
    MutableStructDefinition,
    AbstractDefinition,
    FieldExpression,
    AdjointExpression,
    ParametrizedTypeExpression,
    CurlyExpression,
    SubtypeExpression,
    TypeHead,
    WhereExpression,
    WhereClause,
    TypeParameters,
    TypeParameter,
    TypeParameterList,
    SubtypeConstraint,
    SupertypeConstraint,
    ArrowFunctionExpression,
    DoClause,
    JuxtapositionExpression,
    LetStatement,
    BroadcastCallExpression,
    CompoundStatement,
    LineComment,
    BlockComment,
    BooleanLiteral,
    TernaryExpression,
    PairExpression,
    ConstStatement,
    GlobalStatement,
    LocalStatement,
    LocalDeclaration,
    SplatExpression,
    SplatParameter,
    UnaryTypedExpression,
    KeywordArgument,
    LetExpression,
    LetBindings,
    Semicolon,
    Other(u16),
}

impl NodeKind {
    /// Create NodeKind from a string (for JSON CST and pure Rust parser)
    pub fn from_string(s: &str) -> Self {
        match s {
            "source_file" => NodeKind::SourceFile,
            "function_definition" => NodeKind::FunctionDefinition,
            "short_function_definition" => NodeKind::ShortFunctionDefinition,
            "assignment" => NodeKind::Assignment,
            "compound_assignment_expression" => NodeKind::CompoundAssignment,
            "return_statement" => NodeKind::ReturnStatement,
            "for_statement" => NodeKind::ForStatement,
            "while_statement" => NodeKind::WhileStatement,
            "if_statement" => NodeKind::IfStatement,
            "try_statement" => NodeKind::TryStatement,
            "catch_clause" => NodeKind::CatchClause,
            "else_clause" | "elseif_clause" => NodeKind::ElseClause,
            "finally_clause" => NodeKind::FinallyClause,
            "break_statement" => NodeKind::BreakStatement,
            "continue_statement" => NodeKind::ContinueStatement,
            "block" | "begin_block" => NodeKind::Block,
            "binary_expression" => NodeKind::BinaryExpression,
            "unary_expression" => NodeKind::UnaryExpression,
            "call_expression" => NodeKind::CallExpression,
            "range_expression" => NodeKind::RangeExpression,
            "tuple_expression" => NodeKind::TupleExpression,
            "argument_list" => NodeKind::ArgumentList,
            "parenthesized_expression" => NodeKind::ParenthesizedExpression,
            "for_binding" => NodeKind::ForBinding,
            "identifier" => NodeKind::Identifier,
            "integer_literal" => NodeKind::IntegerLiteral,
            "float_literal" => NodeKind::FloatLiteral,
            "string_literal" | "interpolated_string_literal" => NodeKind::StringLiteral,
            "character_literal" => NodeKind::CharacterLiteral,
            "string_interpolation" | "interpolation" | "interpolation_expression" => {
                NodeKind::StringInterpolation
            }
            "content" | "string_content" => NodeKind::StringContent,
            "macro_expression" | "macrocall_expression" => NodeKind::MacroCall,
            "macro_identifier" => NodeKind::MacroIdentifier,
            "macro_argument_list" => NodeKind::MacroArgumentList,
            "quote_statement" => NodeKind::QuoteStatement,
            "quote_expression" => NodeKind::QuoteExpression,
            "using_statement" => NodeKind::UsingStatement,
            "import_statement" => NodeKind::ImportStatement,
            "export_statement" => NodeKind::ExportStatement,
            "public_statement" => NodeKind::PublicStatement,
            "module_definition" => NodeKind::ModuleDefinition,
            "baremodule_definition" => NodeKind::BaremoduleDefinition,
            "macro_definition" => NodeKind::MacroDefinition,
            "operator" => NodeKind::Operator,
            "vector_expression" => NodeKind::VectorExpression,
            "matrix_expression" => NodeKind::MatrixExpression,
            "matrix_row" => NodeKind::MatrixRow,
            "index_expression" => NodeKind::IndexExpression,
            "comprehension_expression" | "array_comprehension_expression" => {
                NodeKind::ComprehensionExpression
            }
            "generator_expression" | "generator" => NodeKind::GeneratorExpression,
            "for_clause" => NodeKind::ForClause,
            "if_clause" => NodeKind::IfClause,
            "typed_parameter" => NodeKind::TypedParameter,
            "parameter" => NodeKind::Parameter,
            "kw_parameter" => NodeKind::KwParameter,
            "parameter_list" => NodeKind::ParameterList,
            "type_clause" => NodeKind::TypeClause,
            "typed_expression" => NodeKind::TypedExpression,
            "signature" => NodeKind::Signature,
            "struct_definition" => NodeKind::StructDefinition,
            "mutable_struct_definition" => NodeKind::MutableStructDefinition,
            "abstract_definition" => NodeKind::AbstractDefinition,
            "field_expression" => NodeKind::FieldExpression,
            "adjoint_expression" | "prime_expression" => NodeKind::AdjointExpression,
            "parametrized_type_expression" => NodeKind::ParametrizedTypeExpression,
            "curly_expression" => NodeKind::CurlyExpression,
            "subtype_expression" => NodeKind::SubtypeExpression,
            "type_head" => NodeKind::TypeHead,
            "where_expression" => NodeKind::WhereExpression,
            "where_clause" => NodeKind::WhereClause,
            "type_parameters" => NodeKind::TypeParameters,
            "type_parameter" => NodeKind::TypeParameter,
            "type_parameter_list" => NodeKind::TypeParameterList,
            "subtype_constraint" => NodeKind::SubtypeConstraint,
            "supertype_constraint" => NodeKind::SupertypeConstraint,
            "arrow_function_expression" => NodeKind::ArrowFunctionExpression,
            "do_clause" | "do_expression" => NodeKind::DoClause,
            "juxtaposition_expression" | "coefficient_expression" => {
                NodeKind::JuxtapositionExpression
            }
            "let_statement" => NodeKind::LetStatement,
            "broadcast_call_expression" => NodeKind::BroadcastCallExpression,
            "compound_statement" => NodeKind::CompoundStatement,
            "line_comment" => NodeKind::LineComment,
            "block_comment" => NodeKind::BlockComment,
            "boolean_literal" => NodeKind::BooleanLiteral,
            "ternary_expression" => NodeKind::TernaryExpression,
            "pair_expression" => NodeKind::PairExpression,
            "const_statement" | "const_declaration" => NodeKind::ConstStatement,
            "global_statement" | "global_declaration" => NodeKind::GlobalStatement,
            "local_statement" => NodeKind::LocalStatement,
            "local_declaration" => NodeKind::LocalDeclaration,
            "splat_expression" => NodeKind::SplatExpression,
            "splat_parameter" => NodeKind::SplatParameter,
            "unary_typed_expression" => NodeKind::UnaryTypedExpression,
            "keyword_argument" => NodeKind::KeywordArgument,
            "let_expression" => NodeKind::LetExpression,
            "let_bindings" => NodeKind::LetBindings,
            ";" => NodeKind::Semicolon,
            _ => NodeKind::Other(0),
        }
    }

    /// Convert from subset_julia_vm_parser NodeKind
    pub fn from_parser_kind(kind: subset_julia_vm_parser::NodeKind) -> Self {
        // Use the string representation for conversion
        NodeKind::from_string(kind.as_str())
    }
}

// ============================================================================
// Node type - wrapper for pure Rust parser
// ============================================================================

/// Node type for pure Rust parser.
#[derive(Clone, Copy)]
pub struct Node<'a> {
    /// Reference to the CstNode
    pub node: &'a subset_julia_vm_parser::CstNode,
    /// Source code for text extraction
    pub source: &'a str,
}

impl<'a> Node<'a> {
    /// Create a new Node
    pub fn new(node: &'a subset_julia_vm_parser::CstNode, source: &'a str) -> Self {
        Self { node, source }
    }

    /// Get the raw node kind string (for debugging and direct comparison)
    pub fn kind(&self) -> &str {
        self.node.kind.as_str()
    }

    /// Check if this is a named node (not anonymous like punctuation)
    pub fn is_named(&self) -> bool {
        self.node.is_named
    }

    /// Get the start byte offset
    pub fn start_byte(&self) -> usize {
        self.node.span.start
    }

    /// Get the end byte offset
    pub fn end_byte(&self) -> usize {
        self.node.span.end
    }

    /// Get the number of children
    pub fn child_count(&self) -> usize {
        self.node.children.len()
    }

    /// Get a child by index
    pub fn child(&self, index: usize) -> Option<Node<'a>> {
        self.node.children.get(index).map(|n| Node {
            node: n,
            source: self.source,
        })
    }

    /// Get a unique node ID (for debugging/comparison)
    pub fn id(&self) -> usize {
        self.node as *const _ as usize
    }
}

// ============================================================================
// CstWalker - walker for pure Rust parser
// ============================================================================

/// Walker for traversing and querying CST nodes.
pub struct CstWalker<'a> {
    _source: std::marker::PhantomData<&'a str>,
}

impl<'a> CstWalker<'a> {
    pub fn new(_source: &'a str) -> Self {
        Self {
            _source: std::marker::PhantomData,
        }
    }

    /// Get the NodeKind of a node
    pub fn kind(&self, node: &Node<'a>) -> NodeKind {
        NodeKind::from_parser_kind(node.node.kind)
    }

    /// Get the source span of a node
    pub fn span(&self, node: &Node<'a>) -> Span {
        Span::from_parser_span(&node.node.span)
    }

    /// Get the text content of a node
    pub fn text(&self, node: &Node<'a>) -> &'a str {
        node.node.text_from_source(node.source)
    }

    /// Get a child by field name
    pub fn child_by_field(&self, node: &Node<'a>, field: &str) -> Option<Node<'a>> {
        node.node.child_by_field(field).map(|n| Node {
            node: n,
            source: node.source,
        })
    }

    /// Get all children of a node
    pub fn children(&self, node: &Node<'a>) -> Vec<Node<'a>> {
        node.node
            .children
            .iter()
            .map(|n| Node {
                node: n,
                source: node.source,
            })
            .collect()
    }

    /// Get all named children of a node (excludes punctuation)
    pub fn named_children(&self, node: &Node<'a>) -> Vec<Node<'a>> {
        node.node
            .children
            .iter()
            .filter(|n| n.is_named)
            .map(|n| Node {
                node: n,
                source: node.source,
            })
            .collect()
    }

    /// Find a child with the given NodeKind
    pub fn find_child(&self, node: &Node<'a>, kind: NodeKind) -> Option<Node<'a>> {
        self.children(node)
            .into_iter()
            .find(|&child| self.kind(&child) == kind)
    }
}
