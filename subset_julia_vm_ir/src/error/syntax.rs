use crate::span::Span;
use std::fmt;

#[derive(Debug, Clone)]
pub struct SyntaxIssue {
    pub span: Span,
    pub text: String,
}

#[derive(Debug)]
pub enum SyntaxError {
    ParseFailed(String),
    ErrorNodes(Vec<SyntaxIssue>),
}

impl SyntaxError {
    pub fn parse_failed(message: impl Into<String>) -> Self {
        Self::ParseFailed(message.into())
    }

    pub fn from_issues(issues: Vec<SyntaxIssue>) -> Self {
        Self::ErrorNodes(issues)
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyntaxError::ParseFailed(message) => write!(f, "Parse failed: {}", message),
            SyntaxError::ErrorNodes(issues) => {
                if let Some(first) = issues.first() {
                    write!(
                        f,
                        "Syntax errors: {} issue(s): {}",
                        issues.len(),
                        first.text
                    )
                } else {
                    write!(f, "Syntax errors: 0 issue(s)")
                }
            }
        }
    }
}

impl std::error::Error for SyntaxError {}
