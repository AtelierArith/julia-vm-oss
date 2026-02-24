use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedFeatureKind {
    MacroCall,
    MacroDefinition,
    UsingStatement,
    ImportStatement,
    ModuleDefinition,
    FunctionDefinition,
    StructDefinition,
    IfStatement,
    WhileStatement,
    BreakStatement,
    ContinueStatement,
    UnsupportedOperator(String),
    UnsupportedAssignmentTarget,
    UnsupportedCallTarget,
    UnsupportedForBinding,
    UnsupportedRange,
    UnsupportedExpression(String),
    // Array-related errors
    MalformedMatrix,
    ArraySlicing,
    Comprehension,
    EmptyArray,
    NestedArray,
    // Include-related
    IncludeCall(String), // include("path") - path argument
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedFeature {
    pub kind: UnsupportedFeatureKind,
    pub span: Span,
    pub hint: Option<String>,
}

impl UnsupportedFeature {
    pub fn new(kind: UnsupportedFeatureKind, span: Span) -> Self {
        Self {
            kind,
            span,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl std::fmt::Display for UnsupportedFeatureKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MacroCall => write!(f, "macro call"),
            Self::MacroDefinition => write!(f, "macro definition"),
            Self::UsingStatement => write!(f, "using statement"),
            Self::ImportStatement => write!(f, "import statement"),
            Self::ModuleDefinition => write!(f, "module definition"),
            Self::FunctionDefinition => write!(f, "function definition"),
            Self::StructDefinition => write!(f, "struct definition"),
            Self::IfStatement => write!(f, "if statement"),
            Self::WhileStatement => write!(f, "while statement"),
            Self::BreakStatement => write!(f, "break statement"),
            Self::ContinueStatement => write!(f, "continue statement"),
            Self::UnsupportedOperator(op) => write!(f, "unsupported operator: {}", op),
            Self::UnsupportedAssignmentTarget => write!(f, "unsupported assignment target"),
            Self::UnsupportedCallTarget => write!(f, "unsupported call target"),
            Self::UnsupportedForBinding => write!(f, "unsupported for binding"),
            Self::UnsupportedRange => write!(f, "unsupported range expression"),
            Self::UnsupportedExpression(expr) => write!(f, "unsupported expression: {}", expr),
            Self::MalformedMatrix => write!(f, "malformed matrix literal"),
            Self::ArraySlicing => write!(f, "array slicing"),
            Self::Comprehension => write!(f, "array comprehension"),
            Self::EmptyArray => write!(f, "empty array literal"),
            Self::NestedArray => write!(f, "nested array"),
            Self::IncludeCall(path) => write!(f, "include(\"{}\")", path),
            Self::Other(message) => write!(f, "{}", message),
        }
    }
}

impl std::fmt::Display for UnsupportedFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unsupported feature: {}", self.kind)?;
        if let Some(hint) = &self.hint {
            write!(f, " (hint: {})", hint)?;
        }
        Ok(())
    }
}

impl std::error::Error for UnsupportedFeature {}
