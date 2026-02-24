//! NodeKind conversion methods (from_str, as_str)

use std::convert::Infallible;
use std::str::FromStr;

use super::NodeKind;

impl FromStr for NodeKind {
    type Err = Infallible;

    /// Convert from tree-sitter node type string
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "source_file" => NodeKind::SourceFile,
            "block" => NodeKind::Block,

            // Definitions
            "module_definition" => NodeKind::ModuleDefinition,
            "baremodule_definition" => NodeKind::BaremoduleDefinition,
            "abstract_definition" => NodeKind::AbstractDefinition,
            "primitive_definition" => NodeKind::PrimitiveDefinition,
            "struct_definition" => NodeKind::StructDefinition,
            "mutable_struct_definition" => NodeKind::MutableStructDefinition,
            "function_definition" => NodeKind::FunctionDefinition,
            "short_function_definition" => NodeKind::ShortFunctionDefinition,
            "macro_definition" => NodeKind::MacroDefinition,
            "signature" => NodeKind::Signature,
            "type_head" => NodeKind::TypeHead,
            "parameter_list" => NodeKind::ParameterList,
            "parameter" => NodeKind::Parameter,
            "kw_parameter" => NodeKind::KwParameter,
            "splat_parameter" => NodeKind::SplatParameter,
            "type_parameters" => NodeKind::TypeParameters,
            "type_parameter" => NodeKind::TypeParameter,
            "type_parameter_list" => NodeKind::TypeParameterList,
            "subtype_constraint" => NodeKind::SubtypeConstraint,
            "supertype_constraint" => NodeKind::SupertypeConstraint,
            "where_clause" => NodeKind::WhereClause,

            // Statements
            "compound_statement" => NodeKind::CompoundStatement,
            "begin_block" => NodeKind::BeginBlock,
            "let_statement" => NodeKind::LetStatement,
            "let_expression" => NodeKind::LetExpression,
            "let_bindings" => NodeKind::LetBindings,
            "if_statement" => NodeKind::IfStatement,
            "elseif_clause" => NodeKind::ElseifClause,
            "else_clause" => NodeKind::ElseClause,
            "try_statement" => NodeKind::TryStatement,
            "catch_clause" => NodeKind::CatchClause,
            "finally_clause" => NodeKind::FinallyClause,
            "for_statement" => NodeKind::ForStatement,
            "while_statement" => NodeKind::WhileStatement,
            "break_statement" => NodeKind::BreakStatement,
            "continue_statement" => NodeKind::ContinueStatement,
            "return_statement" => NodeKind::ReturnStatement,
            "const_statement" => NodeKind::ConstStatement,
            "const_declaration" => NodeKind::ConstDeclaration,
            "global_statement" => NodeKind::GlobalStatement,
            "global_declaration" => NodeKind::GlobalDeclaration,
            "local_statement" => NodeKind::LocalStatement,
            "local_declaration" => NodeKind::LocalDeclaration,
            "export_statement" => NodeKind::ExportStatement,
            "public_statement" => NodeKind::PublicStatement,
            "import_statement" => NodeKind::ImportStatement,
            "using_statement" => NodeKind::UsingStatement,
            "import_alias" => NodeKind::ImportAlias,
            "import_path" => NodeKind::ImportPath,
            "import_list" => NodeKind::ImportList,

            // Expressions
            "assignment" => NodeKind::Assignment,
            "compound_assignment_expression" => NodeKind::CompoundAssignmentExpression,
            "binary_expression" => NodeKind::BinaryExpression,
            "unary_expression" => NodeKind::UnaryExpression,
            "ternary_expression" => NodeKind::TernaryExpression,
            "call_expression" => NodeKind::CallExpression,
            "broadcast_call_expression" => NodeKind::BroadcastCallExpression,
            "do_clause" => NodeKind::DoClause,
            "arrow_function_expression" => NodeKind::ArrowFunctionExpression,
            "juxtaposition_expression" => NodeKind::JuxtapositionExpression,
            "range_expression" => NodeKind::RangeExpression,
            "splat_expression" => NodeKind::SplatExpression,
            "typed_expression" => NodeKind::TypedExpression,
            "unary_typed_expression" => NodeKind::UnaryTypedExpression,
            "where_expression" => NodeKind::WhereExpression,
            "macrocall_expression" => NodeKind::MacrocallExpression,
            "macro_argument_list" => NodeKind::MacroArgumentList,

            // Primary expressions
            "identifier" => NodeKind::Identifier,
            "boolean_literal" => NodeKind::BooleanLiteral,
            "parenthesized_expression" => NodeKind::ParenthesizedExpression,
            "tuple_expression" => NodeKind::TupleExpression,
            "curly_expression" => NodeKind::CurlyExpression,
            "adjoint_expression" => NodeKind::AdjointExpression,
            "field_expression" => NodeKind::FieldExpression,
            "index_expression" => NodeKind::IndexExpression,
            "parametrized_type_expression" => NodeKind::ParametrizedTypeExpression,
            "quote_expression" => NodeKind::QuoteExpression,

            // Arrays
            "vector_expression" => NodeKind::VectorExpression,
            "matrix_expression" => NodeKind::MatrixExpression,
            "matrix_row" => NodeKind::MatrixRow,
            "comprehension_expression" => NodeKind::ComprehensionExpression,
            "generator" => NodeKind::Generator,
            "for_clause" => NodeKind::ForClause,
            "if_clause" => NodeKind::IfClause,
            "for_binding" => NodeKind::ForBinding,

            // Literals
            "integer_literal" => NodeKind::IntegerLiteral,
            "float_literal" => NodeKind::FloatLiteral,
            "string_literal" => NodeKind::StringLiteral,
            "character_literal" => NodeKind::CharacterLiteral,
            "command_literal" => NodeKind::CommandLiteral,
            "prefixed_string_literal" => NodeKind::PrefixedStringLiteral,
            "content" => NodeKind::Content,
            "string_interpolation" => NodeKind::StringInterpolation,

            // Operators & macros
            "operator" => NodeKind::Operator,
            ";" => NodeKind::Semicolon,
            "macro_identifier" => NodeKind::MacroIdentifier,

            // Type system
            "typed_parameter" => NodeKind::TypedParameter,
            "subtype_expression" => NodeKind::SubtypeExpression,

            // Arguments
            "argument_list" => NodeKind::ArgumentList,

            // Comments
            "line_comment" => NodeKind::LineComment,
            "block_comment" => NodeKind::BlockComment,

            // Other
            "keyword_argument" => NodeKind::KeywordArgument,
            "ERROR" => NodeKind::Error,

            _ => NodeKind::Unknown,
        })
    }
}

impl NodeKind {
    /// Convert to tree-sitter compatible string
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::SourceFile => "source_file",
            NodeKind::Block => "block",

            NodeKind::ModuleDefinition => "module_definition",
            NodeKind::BaremoduleDefinition => "baremodule_definition",
            NodeKind::AbstractDefinition => "abstract_definition",
            NodeKind::PrimitiveDefinition => "primitive_definition",
            NodeKind::StructDefinition => "struct_definition",
            NodeKind::MutableStructDefinition => "mutable_struct_definition",
            NodeKind::FunctionDefinition => "function_definition",
            NodeKind::ShortFunctionDefinition => "short_function_definition",
            NodeKind::MacroDefinition => "macro_definition",
            NodeKind::Signature => "signature",
            NodeKind::TypeHead => "type_head",
            NodeKind::ParameterList => "parameter_list",
            NodeKind::Parameter => "parameter",
            NodeKind::KwParameter => "kw_parameter",
            NodeKind::SplatParameter => "splat_parameter",
            NodeKind::TypeParameters => "type_parameters",
            NodeKind::TypeParameter => "type_parameter",
            NodeKind::TypeParameterList => "type_parameter_list",
            NodeKind::SubtypeConstraint => "subtype_constraint",
            NodeKind::SupertypeConstraint => "supertype_constraint",
            NodeKind::WhereClause => "where_clause",

            NodeKind::CompoundStatement => "compound_statement",
            NodeKind::BeginBlock => "begin_block",
            NodeKind::LetStatement => "let_statement",
            NodeKind::LetExpression => "let_expression",
            NodeKind::LetBindings => "let_bindings",
            NodeKind::IfStatement => "if_statement",
            NodeKind::ElseifClause => "elseif_clause",
            NodeKind::ElseClause => "else_clause",
            NodeKind::TryStatement => "try_statement",
            NodeKind::CatchClause => "catch_clause",
            NodeKind::FinallyClause => "finally_clause",
            NodeKind::ForStatement => "for_statement",
            NodeKind::WhileStatement => "while_statement",
            NodeKind::BreakStatement => "break_statement",
            NodeKind::ContinueStatement => "continue_statement",
            NodeKind::ReturnStatement => "return_statement",
            NodeKind::ConstStatement => "const_statement",
            NodeKind::ConstDeclaration => "const_declaration",
            NodeKind::GlobalStatement => "global_statement",
            NodeKind::GlobalDeclaration => "global_declaration",
            NodeKind::LocalStatement => "local_statement",
            NodeKind::LocalDeclaration => "local_declaration",
            NodeKind::ExportStatement => "export_statement",
            NodeKind::PublicStatement => "public_statement",
            NodeKind::ImportStatement => "import_statement",
            NodeKind::UsingStatement => "using_statement",
            NodeKind::ImportAlias => "import_alias",
            NodeKind::ImportPath => "import_path",
            NodeKind::ImportList => "import_list",

            NodeKind::Assignment => "assignment",
            NodeKind::CompoundAssignmentExpression => "compound_assignment_expression",
            NodeKind::BinaryExpression => "binary_expression",
            NodeKind::UnaryExpression => "unary_expression",
            NodeKind::TernaryExpression => "ternary_expression",
            NodeKind::CallExpression => "call_expression",
            NodeKind::BroadcastCallExpression => "broadcast_call_expression",
            NodeKind::DoClause => "do_clause",
            NodeKind::ArrowFunctionExpression => "arrow_function_expression",
            NodeKind::JuxtapositionExpression => "juxtaposition_expression",
            NodeKind::RangeExpression => "range_expression",
            NodeKind::SplatExpression => "splat_expression",
            NodeKind::TypedExpression => "typed_expression",
            NodeKind::UnaryTypedExpression => "unary_typed_expression",
            NodeKind::WhereExpression => "where_expression",
            NodeKind::MacrocallExpression => "macrocall_expression",
            NodeKind::MacroArgumentList => "macro_argument_list",

            NodeKind::Identifier => "identifier",
            NodeKind::BooleanLiteral => "boolean_literal",
            NodeKind::ParenthesizedExpression => "parenthesized_expression",
            NodeKind::TupleExpression => "tuple_expression",
            NodeKind::CurlyExpression => "curly_expression",
            NodeKind::AdjointExpression => "adjoint_expression",
            NodeKind::FieldExpression => "field_expression",
            NodeKind::IndexExpression => "index_expression",
            NodeKind::ParametrizedTypeExpression => "parametrized_type_expression",
            NodeKind::QuoteExpression => "quote_expression",

            NodeKind::VectorExpression => "vector_expression",
            NodeKind::MatrixExpression => "matrix_expression",
            NodeKind::MatrixRow => "matrix_row",
            NodeKind::ComprehensionExpression => "comprehension_expression",
            NodeKind::Generator => "generator",
            NodeKind::ForClause => "for_clause",
            NodeKind::IfClause => "if_clause",
            NodeKind::ForBinding => "for_binding",

            NodeKind::IntegerLiteral => "integer_literal",
            NodeKind::FloatLiteral => "float_literal",
            NodeKind::StringLiteral => "string_literal",
            NodeKind::CharacterLiteral => "character_literal",
            NodeKind::CommandLiteral => "command_literal",
            NodeKind::PrefixedStringLiteral => "prefixed_string_literal",
            NodeKind::Content => "content",
            NodeKind::StringInterpolation => "string_interpolation",

            NodeKind::Operator => "operator",
            NodeKind::Semicolon => ";",
            NodeKind::MacroIdentifier => "macro_identifier",

            NodeKind::TypedParameter => "typed_parameter",
            NodeKind::SubtypeExpression => "subtype_expression",

            NodeKind::ArgumentList => "argument_list",

            NodeKind::LineComment => "line_comment",
            NodeKind::BlockComment => "block_comment",

            NodeKind::KeywordArgument => "keyword_argument",
            NodeKind::Error => "ERROR",
            NodeKind::Unknown => "unknown",
        }
    }
}
