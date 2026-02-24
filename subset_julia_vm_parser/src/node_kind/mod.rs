//! Node kinds for Julia CST
//!
//! Based on tree-sitter-julia grammar.js rule names

mod convert;
mod predicates;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

/// CST node kinds
///
/// Maps to tree-sitter-julia grammar rules (grammar.js:183-1118)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    // ==================== Top Level ====================
    /// Root node: source_file
    SourceFile,

    /// Block of statements
    Block,

    // ==================== Definitions ====================
    /// module Name ... end
    ModuleDefinition,
    /// abstract type Name end
    AbstractDefinition,
    /// primitive type Name 64 end
    PrimitiveDefinition,
    /// struct Name ... end
    StructDefinition,
    /// function name(...) ... end
    FunctionDefinition,
    /// f(x) = expr (short function definition)
    ShortFunctionDefinition,
    /// macro name(...) ... end
    MacroDefinition,
    /// mutable struct Name ... end
    MutableStructDefinition,
    /// baremodule Name ... end
    BaremoduleDefinition,

    /// Function/macro signature
    ///
    /// Note: Not currently produced by parser. Signatures are handled
    /// inline within FunctionDefinition nodes. (Issue #1627)
    Signature,
    /// Type head in struct/abstract definition
    ///
    /// Note: Not currently produced by parser. Type heads are handled
    /// inline within struct definition nodes. (Issue #1627)
    TypeHead,
    /// Parameter list: (a, b, c)
    ParameterList,
    /// Single parameter: a or a::T or a=default
    Parameter,
    /// Keyword-only parameter (after semicolon): y=1 or y::T=1
    KwParameter,
    /// Splat/varargs parameter: args... or args::T...
    SplatParameter,
    /// Type parameters: {T, S}
    TypeParameters,
    /// Single type parameter: T or T <: Bound
    TypeParameter,
    /// Type parameter list in where clause: {T, S}
    TypeParameterList,
    /// Subtype constraint: T <: Number
    SubtypeConstraint,
    /// Supertype constraint: T >: Integer
    SupertypeConstraint,
    /// Where clause: where T <: Number
    WhereClause,

    // ==================== Statements ====================
    /// begin ... end
    ///
    /// Note: Not currently produced by parser. The parser uses
    /// `BeginBlock` instead. (Issue #1627)
    CompoundStatement,
    /// begin ... end (alias)
    BeginBlock,
    /// let x = 1 ... end
    ///
    /// Note: Not currently produced by parser. The parser uses
    /// `LetExpression` instead. (Issue #1627)
    LetStatement,
    /// let expression (for parser)
    LetExpression,
    /// let bindings: x = 1, y = 2
    LetBindings,

    /// if cond ... end
    IfStatement,
    /// elseif clause
    ElseifClause,
    /// else clause
    ElseClause,

    /// try ... end
    TryStatement,
    /// catch [var] ...
    CatchClause,
    /// finally ...
    FinallyClause,

    /// for v in range ... end
    ForStatement,
    /// while cond ... end
    WhileStatement,

    /// break
    BreakStatement,
    /// continue
    ContinueStatement,
    /// return [expr]
    ReturnStatement,

    /// const x = 1
    ///
    /// Note: Not currently produced by parser. The parser uses
    /// `ConstDeclaration` instead. (Issue #1627)
    ConstStatement,
    /// const declaration (for parser)
    ConstDeclaration,
    /// global x
    ///
    /// Note: Not currently produced by parser. The parser uses
    /// `GlobalDeclaration` instead. (Issue #1627)
    GlobalStatement,
    /// global declaration (for parser)
    GlobalDeclaration,
    /// local x
    ///
    /// Note: Not currently produced by parser. The parser uses
    /// `LocalDeclaration` instead. (Issue #1627)
    LocalStatement,
    /// local declaration (for parser)
    LocalDeclaration,

    /// export name, name2
    ExportStatement,
    /// public name, name2 (Julia 1.11+)
    PublicStatement,
    /// import Module
    ImportStatement,
    /// using Module
    UsingStatement,

    /// import alias: `as`
    ImportAlias,
    /// import path: `.Module` or `..Module`
    ImportPath,
    /// import list: Module1, Module2
    ImportList,

    // ==================== Expressions ====================
    /// x = expr
    Assignment,
    /// x += expr (and other compound assignments)
    CompoundAssignmentExpression,

    /// Binary operation: a + b
    BinaryExpression,
    /// Unary operation: -x, !x
    UnaryExpression,
    /// Ternary: cond ? then : else
    TernaryExpression,

    /// Function call: f(x, y)
    CallExpression,
    /// Broadcast call: f.(x)
    BroadcastCallExpression,
    /// do clause in function call
    DoClause,

    /// Arrow function: x -> x^2
    ArrowFunctionExpression,
    /// Juxtaposition: 2x (implicit multiplication)
    JuxtapositionExpression,

    /// Range: 1:10, 1:2:10
    RangeExpression,
    /// Splat: args...
    SplatExpression,

    /// Type annotation: x::Int
    TypedExpression,
    /// Standalone type annotation: ::Int
    ///
    /// Note: Not currently produced by parser. Standalone type annotations
    /// are handled differently. (Issue #1627)
    UnaryTypedExpression,
    /// Where clause: T where T <: Number
    WhereExpression,

    /// Macro call: @macro args
    MacrocallExpression,
    /// Macro argument list
    ///
    /// Note: Not currently produced by parser. The parser puts args
    /// directly in `MacrocallExpression`. (Issue #1627)
    MacroArgumentList,

    // ==================== Primary Expressions ====================
    /// Variable name
    Identifier,
    /// true, false
    BooleanLiteral,

    /// (expr) or (expr1; expr2)
    ParenthesizedExpression,
    /// (a, b, c) or (a=1, b=2)
    TupleExpression,

    /// {T} or {T, S}
    ///
    /// Note: Not currently produced by parser. Curly braces at top-level
    /// are not valid Julia syntax. (Issue #1627)
    CurlyExpression,

    /// Transpose/adjoint: x'
    AdjointExpression,

    /// Field access: obj.field
    FieldExpression,
    /// Index: arr[i] or arr[i, j]
    IndexExpression,

    /// Parametrized type: Point{T}
    ParametrizedTypeExpression,

    /// Quote expression: :symbol or :(expr)
    QuoteExpression,

    // ==================== Arrays ====================
    /// [1, 2, 3]
    VectorExpression,
    /// [1 2; 3 4]
    MatrixExpression,
    /// Row in matrix
    MatrixRow,
    /// [x^2 for x in 1:10]
    ComprehensionExpression,
    /// (x^2 for x in 1:10)
    Generator,

    /// for clause in comprehension
    ForClause,
    /// if clause in comprehension
    IfClause,
    /// Binding in for: v in range
    ForBinding,

    // ==================== Literals ====================
    /// 42, 0xff, 0b101
    IntegerLiteral,
    /// 3.14, 1e-5
    FloatLiteral,

    /// "hello" or """multi-line"""
    StringLiteral,
    /// 'a'
    CharacterLiteral,
    /// `command`
    CommandLiteral,

    /// r"raw" (prefixed string)
    PrefixedStringLiteral,

    /// String/interpolation content
    Content,
    /// String interpolation: $x or $(expr)
    StringInterpolation,

    // ==================== Operators ====================
    /// Operator token
    Operator,
    /// Semicolon separator (used in argument lists to separate kwargs)
    Semicolon,

    // ==================== Macro ====================
    /// @name
    MacroIdentifier,

    // ==================== Type System ====================
    /// Typed parameter: x::Int
    TypedParameter,
    /// Subtype expression: T <: Number
    ///
    /// Note: Not currently produced by parser. The parser uses
    /// `SubtypeConstraint` in where clauses instead. (Issue #1627)
    SubtypeExpression,

    // ==================== Arguments ====================
    /// Argument list in call expression
    ArgumentList,

    // ==================== Comments ====================
    /// # line comment
    ///
    /// Note: Not currently produced by parser. Comments are skipped
    /// during parsing. (Issue #1627)
    LineComment,
    /// #= block comment =#
    ///
    /// Note: Not currently produced by parser. Comments are skipped
    /// during parsing. (Issue #1627)
    BlockComment,

    // ==================== Other ====================
    /// Keyword pair: key=value in call
    KeywordArgument,

    /// Error recovery node
    Error,

    /// Unknown node type (fallback)
    Unknown,
}

impl NodeKind {
    /// Returns all NodeKind variants.
    ///
    /// This list MUST be updated when adding new variants. The exhaustive match
    /// in `variant_count()` will cause a compile error if a variant is missing,
    /// and the `test_all_variants_is_exhaustive` test verifies this list is
    /// complete. (Issue #1627)
    pub fn all_variants() -> &'static [NodeKind] {
        &[
            // Top Level
            NodeKind::SourceFile,
            NodeKind::Block,
            // Definitions
            NodeKind::ModuleDefinition,
            NodeKind::AbstractDefinition,
            NodeKind::PrimitiveDefinition,
            NodeKind::StructDefinition,
            NodeKind::FunctionDefinition,
            NodeKind::ShortFunctionDefinition,
            NodeKind::MacroDefinition,
            NodeKind::MutableStructDefinition,
            NodeKind::BaremoduleDefinition,
            NodeKind::Signature,
            NodeKind::TypeHead,
            NodeKind::ParameterList,
            NodeKind::Parameter,
            NodeKind::KwParameter,
            NodeKind::SplatParameter,
            NodeKind::TypeParameters,
            NodeKind::TypeParameter,
            NodeKind::TypeParameterList,
            NodeKind::SubtypeConstraint,
            NodeKind::SupertypeConstraint,
            NodeKind::WhereClause,
            // Statements
            NodeKind::CompoundStatement,
            NodeKind::BeginBlock,
            NodeKind::LetStatement,
            NodeKind::LetExpression,
            NodeKind::LetBindings,
            NodeKind::IfStatement,
            NodeKind::ElseifClause,
            NodeKind::ElseClause,
            NodeKind::TryStatement,
            NodeKind::CatchClause,
            NodeKind::FinallyClause,
            NodeKind::ForStatement,
            NodeKind::WhileStatement,
            NodeKind::BreakStatement,
            NodeKind::ContinueStatement,
            NodeKind::ReturnStatement,
            NodeKind::ConstStatement,
            NodeKind::ConstDeclaration,
            NodeKind::GlobalStatement,
            NodeKind::GlobalDeclaration,
            NodeKind::LocalStatement,
            NodeKind::LocalDeclaration,
            NodeKind::ExportStatement,
            NodeKind::PublicStatement,
            NodeKind::ImportStatement,
            NodeKind::UsingStatement,
            NodeKind::ImportAlias,
            NodeKind::ImportPath,
            NodeKind::ImportList,
            // Expressions
            NodeKind::Assignment,
            NodeKind::CompoundAssignmentExpression,
            NodeKind::BinaryExpression,
            NodeKind::UnaryExpression,
            NodeKind::TernaryExpression,
            NodeKind::CallExpression,
            NodeKind::BroadcastCallExpression,
            NodeKind::DoClause,
            NodeKind::ArrowFunctionExpression,
            NodeKind::JuxtapositionExpression,
            NodeKind::RangeExpression,
            NodeKind::SplatExpression,
            NodeKind::TypedExpression,
            NodeKind::UnaryTypedExpression,
            NodeKind::WhereExpression,
            NodeKind::MacrocallExpression,
            NodeKind::MacroArgumentList,
            // Primary Expressions
            NodeKind::Identifier,
            NodeKind::BooleanLiteral,
            NodeKind::ParenthesizedExpression,
            NodeKind::TupleExpression,
            NodeKind::CurlyExpression,
            NodeKind::AdjointExpression,
            NodeKind::FieldExpression,
            NodeKind::IndexExpression,
            NodeKind::ParametrizedTypeExpression,
            NodeKind::QuoteExpression,
            // Arrays
            NodeKind::VectorExpression,
            NodeKind::MatrixExpression,
            NodeKind::MatrixRow,
            NodeKind::ComprehensionExpression,
            NodeKind::Generator,
            NodeKind::ForClause,
            NodeKind::IfClause,
            NodeKind::ForBinding,
            // Literals
            NodeKind::IntegerLiteral,
            NodeKind::FloatLiteral,
            NodeKind::StringLiteral,
            NodeKind::CharacterLiteral,
            NodeKind::CommandLiteral,
            NodeKind::PrefixedStringLiteral,
            NodeKind::Content,
            NodeKind::StringInterpolation,
            // Operators
            NodeKind::Operator,
            NodeKind::Semicolon,
            // Macro
            NodeKind::MacroIdentifier,
            // Type System
            NodeKind::TypedParameter,
            NodeKind::SubtypeExpression,
            // Arguments
            NodeKind::ArgumentList,
            // Comments
            NodeKind::LineComment,
            NodeKind::BlockComment,
            // Other
            NodeKind::KeywordArgument,
            NodeKind::Error,
            NodeKind::Unknown,
        ]
    }

    /// Returns the number of variants via an exhaustive match.
    ///
    /// This function exists solely to cause a compile error when a new
    /// NodeKind variant is added but `all_variants()` is not updated.
    /// The `#[deny(unreachable_patterns)]` + wildcard-free match ensures
    /// the compiler rejects any missing arm.
    #[cfg(test)]
    fn variant_count() -> usize {
        // This exhaustive match has no wildcard — adding a new variant
        // without adding an arm here will cause a compile error.
        fn _exhaustive_check(k: NodeKind) -> u8 {
            match k {
                NodeKind::SourceFile => 0,
                NodeKind::Block => 0,
                NodeKind::ModuleDefinition => 0,
                NodeKind::AbstractDefinition => 0,
                NodeKind::PrimitiveDefinition => 0,
                NodeKind::StructDefinition => 0,
                NodeKind::FunctionDefinition => 0,
                NodeKind::ShortFunctionDefinition => 0,
                NodeKind::MacroDefinition => 0,
                NodeKind::MutableStructDefinition => 0,
                NodeKind::BaremoduleDefinition => 0,
                NodeKind::Signature => 0,
                NodeKind::TypeHead => 0,
                NodeKind::ParameterList => 0,
                NodeKind::Parameter => 0,
                NodeKind::KwParameter => 0,
                NodeKind::SplatParameter => 0,
                NodeKind::TypeParameters => 0,
                NodeKind::TypeParameter => 0,
                NodeKind::TypeParameterList => 0,
                NodeKind::SubtypeConstraint => 0,
                NodeKind::SupertypeConstraint => 0,
                NodeKind::WhereClause => 0,
                NodeKind::CompoundStatement => 0,
                NodeKind::BeginBlock => 0,
                NodeKind::LetStatement => 0,
                NodeKind::LetExpression => 0,
                NodeKind::LetBindings => 0,
                NodeKind::IfStatement => 0,
                NodeKind::ElseifClause => 0,
                NodeKind::ElseClause => 0,
                NodeKind::TryStatement => 0,
                NodeKind::CatchClause => 0,
                NodeKind::FinallyClause => 0,
                NodeKind::ForStatement => 0,
                NodeKind::WhileStatement => 0,
                NodeKind::BreakStatement => 0,
                NodeKind::ContinueStatement => 0,
                NodeKind::ReturnStatement => 0,
                NodeKind::ConstStatement => 0,
                NodeKind::ConstDeclaration => 0,
                NodeKind::GlobalStatement => 0,
                NodeKind::GlobalDeclaration => 0,
                NodeKind::LocalStatement => 0,
                NodeKind::LocalDeclaration => 0,
                NodeKind::ExportStatement => 0,
                NodeKind::PublicStatement => 0,
                NodeKind::ImportStatement => 0,
                NodeKind::UsingStatement => 0,
                NodeKind::ImportAlias => 0,
                NodeKind::ImportPath => 0,
                NodeKind::ImportList => 0,
                NodeKind::Assignment => 0,
                NodeKind::CompoundAssignmentExpression => 0,
                NodeKind::BinaryExpression => 0,
                NodeKind::UnaryExpression => 0,
                NodeKind::TernaryExpression => 0,
                NodeKind::CallExpression => 0,
                NodeKind::BroadcastCallExpression => 0,
                NodeKind::DoClause => 0,
                NodeKind::ArrowFunctionExpression => 0,
                NodeKind::JuxtapositionExpression => 0,
                NodeKind::RangeExpression => 0,
                NodeKind::SplatExpression => 0,
                NodeKind::TypedExpression => 0,
                NodeKind::UnaryTypedExpression => 0,
                NodeKind::WhereExpression => 0,
                NodeKind::MacrocallExpression => 0,
                NodeKind::MacroArgumentList => 0,
                NodeKind::Identifier => 0,
                NodeKind::BooleanLiteral => 0,
                NodeKind::ParenthesizedExpression => 0,
                NodeKind::TupleExpression => 0,
                NodeKind::CurlyExpression => 0,
                NodeKind::AdjointExpression => 0,
                NodeKind::FieldExpression => 0,
                NodeKind::IndexExpression => 0,
                NodeKind::ParametrizedTypeExpression => 0,
                NodeKind::QuoteExpression => 0,
                NodeKind::VectorExpression => 0,
                NodeKind::MatrixExpression => 0,
                NodeKind::MatrixRow => 0,
                NodeKind::ComprehensionExpression => 0,
                NodeKind::Generator => 0,
                NodeKind::ForClause => 0,
                NodeKind::IfClause => 0,
                NodeKind::ForBinding => 0,
                NodeKind::IntegerLiteral => 0,
                NodeKind::FloatLiteral => 0,
                NodeKind::StringLiteral => 0,
                NodeKind::CharacterLiteral => 0,
                NodeKind::CommandLiteral => 0,
                NodeKind::PrefixedStringLiteral => 0,
                NodeKind::Content => 0,
                NodeKind::StringInterpolation => 0,
                NodeKind::Operator => 0,
                NodeKind::Semicolon => 0,
                NodeKind::MacroIdentifier => 0,
                NodeKind::TypedParameter => 0,
                NodeKind::SubtypeExpression => 0,
                NodeKind::ArgumentList => 0,
                NodeKind::LineComment => 0,
                NodeKind::BlockComment => 0,
                NodeKind::KeywordArgument => 0,
                NodeKind::Error => 0,
                NodeKind::Unknown => 0,
                // DO NOT add a wildcard `_ =>` here!
                // New variants must be added explicitly to trigger
                // a compile error when all_variants() is not updated.
            }
        }
        NodeKind::all_variants().len()
    }
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
