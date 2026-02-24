//! NodeKind predicate methods

use super::NodeKind;

impl NodeKind {
    /// Check if this is a named node (vs anonymous/punctuation)
    pub fn is_named(&self) -> bool {
        // Almost all nodes are named except for pure punctuation
        !matches!(self, NodeKind::Unknown)
    }

    /// Check if this is a definition node
    pub fn is_definition(&self) -> bool {
        matches!(
            self,
            NodeKind::ModuleDefinition
                | NodeKind::AbstractDefinition
                | NodeKind::PrimitiveDefinition
                | NodeKind::StructDefinition
                | NodeKind::FunctionDefinition
                | NodeKind::MacroDefinition
        )
    }

    /// Check if this is a statement node
    pub fn is_statement(&self) -> bool {
        matches!(
            self,
            NodeKind::IfStatement
                | NodeKind::TryStatement
                | NodeKind::ForStatement
                | NodeKind::WhileStatement
                | NodeKind::BreakStatement
                | NodeKind::ContinueStatement
                | NodeKind::ReturnStatement
                | NodeKind::ConstDeclaration
                | NodeKind::GlobalDeclaration
                | NodeKind::LocalDeclaration
                | NodeKind::ExportStatement
                | NodeKind::PublicStatement
                | NodeKind::ImportStatement
                | NodeKind::UsingStatement
        )
    }

    /// Check if this is an expression node
    ///
    /// Note (Issue #2265): Only include NodeKinds that are actually produced by the parser.
    /// Tree-sitter-compatible variants like UnaryTypedExpression should NOT be included.
    pub fn is_expression(&self) -> bool {
        matches!(
            self,
            NodeKind::Assignment
                | NodeKind::CompoundAssignmentExpression
                | NodeKind::BinaryExpression
                | NodeKind::UnaryExpression
                | NodeKind::TernaryExpression
                | NodeKind::CallExpression
                | NodeKind::BroadcastCallExpression
                | NodeKind::ArrowFunctionExpression
                | NodeKind::JuxtapositionExpression
                | NodeKind::RangeExpression
                | NodeKind::SplatExpression
                | NodeKind::TypedExpression
                // UnaryTypedExpression removed - not produced by parser (Issue #2265)
                | NodeKind::WhereExpression
                | NodeKind::MacrocallExpression
                | NodeKind::Identifier
                | NodeKind::BooleanLiteral
                | NodeKind::ParenthesizedExpression
                | NodeKind::TupleExpression
                | NodeKind::AdjointExpression
                | NodeKind::FieldExpression
                | NodeKind::IndexExpression
                | NodeKind::ParametrizedTypeExpression
                | NodeKind::QuoteExpression
                | NodeKind::BeginBlock
                | NodeKind::LetExpression
                | NodeKind::StringInterpolation
                | NodeKind::VectorExpression
                | NodeKind::MatrixExpression
                | NodeKind::ComprehensionExpression
                | NodeKind::Generator
                | NodeKind::IntegerLiteral
                | NodeKind::FloatLiteral
                | NodeKind::StringLiteral
                | NodeKind::CharacterLiteral
                | NodeKind::CommandLiteral
                | NodeKind::PrefixedStringLiteral
        )
    }

    /// Check if this is a literal node
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            NodeKind::IntegerLiteral
                | NodeKind::FloatLiteral
                | NodeKind::StringLiteral
                | NodeKind::CharacterLiteral
                | NodeKind::BooleanLiteral
                | NodeKind::CommandLiteral
                | NodeKind::PrefixedStringLiteral
        )
    }
}
