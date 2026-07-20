use std::path::PathBuf;

use crate::span::Span;

/// Errors that can occur during include file processing.
#[derive(Debug, Clone)]
pub enum IncludeError {
    /// File not found at the specified path.
    FileNotFound {
        requested_path: String,
        resolved_path: PathBuf,
    },

    /// Circular include detected (file includes itself directly or indirectly).
    CircularInclude {
        path: PathBuf,
        include_chain: Vec<String>,
    },

    /// Parse error in the included file.
    ParseError { file_path: String, message: String },

    /// Lowering error in the included file.
    LowerError { file_path: String, message: String },

    /// I/O error reading the file.
    IoError { file_path: String, message: String },

    /// Include not supported on this platform (iOS/WASM).
    NotSupported { reason: String },

    /// Dynamic path not supported (path must be a string literal).
    DynamicPath { span: Span },
}

impl std::fmt::Display for IncludeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound {
                requested_path,
                resolved_path,
            } => {
                write!(
                    f,
                    "include: file not found '{}' (resolved to '{}')",
                    requested_path,
                    resolved_path.display()
                )
            }
            Self::CircularInclude {
                path,
                include_chain,
            } => {
                write!(
                    f,
                    "include: circular include detected for '{}'\n  include chain: {}",
                    path.display(),
                    include_chain.join(" -> ")
                )
            }
            Self::ParseError { file_path, message } => {
                write!(f, "include: parse error in '{}': {}", file_path, message)
            }
            Self::LowerError { file_path, message } => {
                write!(f, "include: lowering error in '{}': {}", file_path, message)
            }
            Self::IoError { file_path, message } => {
                write!(f, "include: I/O error reading '{}': {}", file_path, message)
            }
            Self::NotSupported { reason } => {
                write!(f, "include: {}", reason)
            }
            Self::DynamicPath { .. } => {
                write!(
                    f,
                    "include: path must be a string literal, not a variable or expression"
                )
            }
        }
    }
}

impl std::error::Error for IncludeError {}
