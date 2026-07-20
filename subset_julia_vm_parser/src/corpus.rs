//! Corpus sweep support for parser differential testing vs upstream Julia
//! (Issue #8614 / #8635).
//!
//! Parses `.jl` files from the upstream `julia/` submodule corpus
//! (parse only — no lowering, no execution) and reports every parse failure
//! as a machine-readable record. Consumed by the `parse_corpus` bin
//! (driven by `scripts/parser_corpus_sweep.sh`) and by the corpus allowlist
//! ratchet test.

use crate::error::ParseError;
use crate::parser;
use std::panic::{self, AssertUnwindSafe};

/// Stack size for per-file parse threads. Some upstream files nest deeply and
/// the parser is recursive-descent, so give it generous headroom instead of
/// crashing the whole sweep on one pathological file.
const PARSE_THREAD_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Maximum characters kept per TSV field before truncation.
const MAX_FIELD_CHARS: usize = 200;

/// A single corpus divergence record: one parse error (or panic) in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusRecord {
    /// Corpus-relative path of the swept file (as passed to the sweep).
    pub file: String,
    /// `start_line:start_col-end_line:end_col` (empty for panics).
    pub span: String,
    /// `ParseError` variant name, `Panic`, or `ReadError`.
    pub error_kind: String,
    /// Source line at the error span (escaped, truncated; empty for panics).
    pub snippet: String,
    /// Full error / panic message (escaped, truncated).
    pub message: String,
}

impl CorpusRecord {
    /// Render as one TSV line (columns: file, span, error_kind, snippet, message).
    pub fn to_tsv(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            self.file, self.span, self.error_kind, self.snippet, self.message
        )
    }
}

/// TSV header line matching [`CorpusRecord::to_tsv`].
pub const TSV_HEADER: &str = "#file\tspan\terror_kind\tsnippet\tmessage";

/// Outcome of sweeping a single corpus file.
#[derive(Debug)]
pub enum FileOutcome {
    /// Parsed cleanly (no recovered errors).
    Ok,
    /// Parse errors (error recovery can report several per file).
    Errors(Vec<CorpusRecord>),
    /// The parser panicked on this file.
    Panic(CorpusRecord),
}

/// Stable name of a [`ParseError`] variant, used as the TSV `error_kind`.
pub fn error_kind_name(error: &ParseError) -> &'static str {
    match error {
        ParseError::UnexpectedToken { .. } => "UnexpectedToken",
        ParseError::UnexpectedEof { .. } => "UnexpectedEof",
        ParseError::InvalidEscape { .. } => "InvalidEscape",
        ParseError::UnterminatedString { .. } => "UnterminatedString",
        ParseError::UnterminatedCommand { .. } => "UnterminatedCommand",
        ParseError::UnterminatedCharacter { .. } => "UnterminatedCharacter",
        ParseError::UnterminatedBlockComment { .. } => "UnterminatedBlockComment",
        ParseError::InvalidNumber { .. } => "InvalidNumber",
        ParseError::InvalidCharacter { .. } => "InvalidCharacter",
        ParseError::MismatchedBrackets { .. } => "MismatchedBrackets",
        ParseError::UnclosedBracket { .. } => "UnclosedBracket",
        ParseError::InvalidSyntax { .. } => "InvalidSyntax",
        ParseError::LexerError { .. } => "LexerError",
    }
}

/// Escape a field for single-line TSV output (tabs/newlines/backslashes),
/// truncating to [`MAX_FIELD_CHARS`] characters.
fn escape_field(text: &str) -> String {
    let mut out = String::new();
    let mut truncated = false;
    for (i, ch) in text.chars().enumerate() {
        if i >= MAX_FIELD_CHARS {
            truncated = true;
            break;
        }
        match ch {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    if truncated {
        out.push_str("...");
    }
    out
}

fn record_for_error(file: &str, source: &str, error: &ParseError) -> CorpusRecord {
    let (span_text, snippet) = match error.span() {
        Some(span) => {
            let line = source
                .lines()
                .nth(span.start_line.saturating_sub(1))
                .unwrap_or("");
            (
                format!(
                    "{}:{}-{}:{}",
                    span.start_line, span.start_column, span.end_line, span.end_column
                ),
                escape_field(line.trim()),
            )
        }
        None => (String::new(), String::new()),
    };
    CorpusRecord {
        file: file.to_string(),
        span: span_text,
        error_kind: error_kind_name(error).to_string(),
        snippet,
        message: escape_field(&error.to_string()),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Parse `source` (labelled `file`) in a dedicated large-stack thread,
/// catching panics so one bad file cannot abort a whole sweep.
pub fn sweep_source(file: &str, source: &str) -> FileOutcome {
    let parse_result = std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("corpus-parse".to_string())
            .stack_size(PARSE_THREAD_STACK_BYTES)
            .spawn_scoped(scope, || {
                panic::catch_unwind(AssertUnwindSafe(|| parser::parse(source)))
            });
        match handle {
            Ok(joinable) => joinable.join().unwrap_or_else(Err),
            // Failing to spawn a thread is a harness problem, not a corpus
            // divergence — surface it as a panic record.
            Err(spawn_error) => Err(Box::new(format!(
                "failed to spawn corpus parse thread: {spawn_error}"
            )) as Box<dyn std::any::Any + Send>),
        }
    });

    match parse_result {
        Ok((_cst, errors)) if errors.is_empty() => FileOutcome::Ok,
        Ok((_cst, errors)) => FileOutcome::Errors(
            errors
                .iter()
                .map(|error| record_for_error(file, source, error))
                .collect(),
        ),
        Err(payload) => FileOutcome::Panic(CorpusRecord {
            file: file.to_string(),
            span: String::new(),
            error_kind: "Panic".to_string(),
            snippet: String::new(),
            message: escape_field(&panic_message(payload.as_ref())),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_sweep_source_ok_on_valid_program() {
        assert!(matches!(
            sweep_source("mem.jl", "f(x) = x + 1"),
            FileOutcome::Ok
        ));
    }

    #[test]
    fn corpus_sweep_source_reports_parse_error_with_span_and_kind() {
        let FileOutcome::Errors(records) = sweep_source("mem.jl", "function f(\nend") else {
            panic!("expected parse errors");
        };
        assert!(!records.is_empty());
        let record = &records[0];
        assert_eq!(record.file, "mem.jl");
        assert!(!record.error_kind.is_empty());
        assert!(!record.span.is_empty());
        // TSV line must stay single-line even with newlines in context.
        assert!(!record.to_tsv().contains('\n'));
    }

    #[test]
    fn corpus_escape_field_escapes_and_truncates() {
        assert_eq!(escape_field("a\tb\nc\\d"), "a\\tb\\nc\\\\d");
        let long = "x".repeat(MAX_FIELD_CHARS + 10);
        let escaped = escape_field(&long);
        assert!(escaped.ends_with("..."));
        assert_eq!(escaped.chars().count(), MAX_FIELD_CHARS + 3);
    }
}
