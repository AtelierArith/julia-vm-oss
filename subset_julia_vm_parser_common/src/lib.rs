//! Canonical CST shared between parser implementations (Issue #11049).
//!
//! This crate implements the common model of the Lezer-compatible parser
//! plan (spec archived in Issue #11225; schema committed at
//! `schemas/canonical-cst.schema.json` in this crate; milestones M0/M1): the Canonical
//! CST node types, spans, node values, diagnostics, and the JSON
//! (de)serialization matching `schemas/canonical-cst.schema.json`:
//!
//! ```json
//! {
//!   "version": 1,
//!   "root": { "kind": "SourceFile", "span": {"start": 0, "end": 42},
//!             "children": [], "value": null, "flags": [] },
//!   "diagnostics": []
//! }
//! ```
//!
//! The Canonical CST is the neutral tree shape used for differential testing
//! between the legacy sjulia parser, the lezer-compatible port, and the
//! lezer-julia oracle (`tools/lezer-oracle.mjs`). Canonicalization rules
//! (spans are UTF-8 byte offsets, anonymous tokens dropped, lezer node-name
//! mapping) are documented in `tools/lezer-oracle.mjs` and must stay in sync
//! with this crate.

use serde::{Deserialize, Serialize};

/// UTF-8 byte span into the source; `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Span { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub fn contains(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

/// Declare a string-backed enum with an `Other(String)` passthrough variant,
/// serialized as a plain JSON string.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant,)+
            /// Passthrough for names outside the canonical catalog (e.g.
            /// lezer node names not yet mapped by the normalizer prototype).
            Other(String),
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                match self {
                    $($name::$variant => $text,)+
                    $name::Other(s) => s,
                }
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                match s {
                    $($text => $name::$variant,)+
                    other => $name::Other(other.to_string()),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Ok($name::from(s.as_str()))
            }
        }
    };
}

string_enum! {
    /// Canonical node kinds (spec `05_canonical_cst.md` §5.4). Names outside
    /// the catalog (lezer passthrough) land in `NodeKind::Other`.
    NodeKind {
        // Structure
        SourceFile => "SourceFile",
        Block => "Block",
        ErrorNode => "ErrorNode",
        // Definition
        FunctionDefinition => "FunctionDefinition",
        ShortFunctionDefinition => "ShortFunctionDefinition",
        MacroDefinition => "MacroDefinition",
        StructDefinition => "StructDefinition",
        MutableStructDefinition => "MutableStructDefinition",
        ModuleDefinition => "ModuleDefinition",
        // Control flow
        IfStatement => "IfStatement",
        ElseIfClause => "ElseIfClause",
        ElseClause => "ElseClause",
        ForStatement => "ForStatement",
        WhileStatement => "WhileStatement",
        TryStatement => "TryStatement",
        CatchClause => "CatchClause",
        FinallyClause => "FinallyClause",
        LetExpression => "LetExpression",
        BeginBlock => "BeginBlock",
        // Expression
        Assignment => "Assignment",
        CompoundAssignment => "CompoundAssignment",
        BinaryExpression => "BinaryExpression",
        UnaryExpression => "UnaryExpression",
        PostfixExpression => "PostfixExpression",
        CallExpression => "CallExpression",
        BroadcastCallExpression => "BroadcastCallExpression",
        IndexExpression => "IndexExpression",
        FieldExpression => "FieldExpression",
        RangeExpression => "RangeExpression",
        TernaryExpression => "TernaryExpression",
        LambdaExpression => "LambdaExpression",
        TypedExpression => "TypedExpression",
        WhereExpression => "WhereExpression",
        DoClause => "DoClause",
        MacroCallExpression => "MacroCallExpression",
        // Collection
        TupleExpression => "TupleExpression",
        VectorExpression => "VectorExpression",
        MatrixExpression => "MatrixExpression",
        ComprehensionExpression => "ComprehensionExpression",
        GeneratorExpression => "GeneratorExpression",
        ForClause => "ForClause",
        IfClause => "IfClause",
        // Leaf
        Identifier => "Identifier",
        Operator => "Operator",
        IntegerLiteral => "IntegerLiteral",
        FloatLiteral => "FloatLiteral",
        StringExpression => "StringExpression",
        StringFragment => "StringFragment",
        Interpolation => "Interpolation",
        CharacterLiteral => "CharacterLiteral",
        BooleanLiteral => "BooleanLiteral",
        NothingLiteral => "NothingLiteral",
    }
}

string_enum! {
    /// Stable diagnostic codes (spec `09_diagnostics.md` §9.2). Tests compare
    /// codes, not message strings.
    DiagnosticCode {
        UnexpectedToken => "UNEXPECTED_TOKEN",
        ExpectedExpression => "EXPECTED_EXPRESSION",
        ExpectedIdentifier => "EXPECTED_IDENTIFIER",
        ExpectedEnd => "EXPECTED_END",
        UnterminatedString => "UNTERMINATED_STRING",
        UnterminatedBlockComment => "UNTERMINATED_BLOCK_COMMENT",
        InvalidCharacterLiteral => "INVALID_CHARACTER_LITERAL",
        InvalidNumericLiteral => "INVALID_NUMERIC_LITERAL",
        InvalidInterpolation => "INVALID_INTERPOLATION",
        MismatchedDelimiter => "MISMATCHED_DELIMITER",
        InvalidAssignmentTarget => "INVALID_ASSIGNMENT_TARGET",
    }
}

/// Leaf node payload (spec §5.3). JSON is externally tagged:
/// `{"Identifier": "x"}`, `{"Operator": "+"}`, ...
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeValue {
    Identifier(String),
    Operator(String),
    IntegerLiteral(String),
    FloatLiteral(String),
    StringFragment(String),
    CharacterLiteral(String),
    Keyword(String),
}

impl NodeValue {
    /// The verbatim source text carried by this value.
    pub fn text(&self) -> &str {
        match self {
            NodeValue::Identifier(s)
            | NodeValue::Operator(s)
            | NodeValue::IntegerLiteral(s)
            | NodeValue::FloatLiteral(s)
            | NodeValue::StringFragment(s)
            | NodeValue::CharacterLiteral(s)
            | NodeValue::Keyword(s) => s,
        }
    }
}

/// One node of the Canonical CST (spec §5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CstNode {
    pub kind: NodeKind,
    pub span: Span,
    #[serde(default)]
    pub children: Vec<CstNode>,
    #[serde(default)]
    pub value: Option<NodeValue>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Parser diagnostic (spec §9.1 / schema `$defs/diagnostic`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    #[serde(default)]
    pub expected: Vec<String>,
    #[serde(default)]
    pub recovery: Option<String>,
}

/// The versioned Canonical CST document (spec §5.7 / JSON schema root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalDocument {
    pub version: u32,
    pub root: CstNode,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// The schema version this crate implements.
pub const CANONICAL_CST_VERSION: u32 = 1;

impl CanonicalDocument {
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

/// One oracle corpus case: a named source snippet plus its oracle document,
/// as emitted by `tools/lezer-oracle-snapshots.mjs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleCase {
    pub name: String,
    pub source: String,
    pub document: CanonicalDocument,
}

/// First structural divergence between two Canonical CSTs, for test output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// Path of `kind[child-index]` segments from the root to the divergence.
    pub path: String,
    /// Human-readable description of the mismatch.
    pub detail: String,
}

fn diverge(path: &str, detail: String) -> Divergence {
    Divergence {
        path: path.to_string(),
        detail,
    }
}

impl CstNode {
    /// Render the tree in the indented style of lezer-julia's `bin/parse.js`,
    /// for readable test-failure output.
    pub fn to_pretty(&self) -> String {
        let mut out = String::new();
        self.pretty_into(&mut out, 0);
        out
    }

    fn pretty_into(&self, out: &mut String, depth: usize) {
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str(self.kind.as_str());
        if let Some(value) = &self.value {
            out.push_str(": ");
            out.push_str(value.text());
        } else {
            out.push_str(&format!(" {}..{}", self.span.start, self.span.end));
        }
        out.push('\n');
        for child in &self.children {
            child.pretty_into(out, depth + 1);
        }
    }

    /// Locate the first divergence between `self` (expected, e.g. the oracle)
    /// and `other` (actual). Returns `None` when the trees are identical on
    /// kind, span, value, and child count. `flags` are not compared.
    pub fn first_divergence(&self, other: &CstNode) -> Option<Divergence> {
        self.diverge_at(other, &mut self.kind.as_str().to_string())
    }

    fn diverge_at(&self, other: &CstNode, path: &mut String) -> Option<Divergence> {
        if self.kind != other.kind {
            return Some(diverge(
                path,
                format!("kind: expected `{}`, got `{}`", self.kind, other.kind),
            ));
        }
        if self.span != other.span {
            return Some(diverge(
                path,
                format!("span: expected {:?}, got {:?}", self.span, other.span),
            ));
        }
        if self.value != other.value {
            return Some(diverge(
                path,
                format!("value: expected {:?}, got {:?}", self.value, other.value),
            ));
        }
        if self.children.len() != other.children.len() {
            return Some(diverge(
                path,
                format!(
                    "child count: expected {}, got {}",
                    self.children.len(),
                    other.children.len()
                ),
            ));
        }
        for (i, (a, b)) in self.children.iter().zip(&other.children).enumerate() {
            let saved = path.len();
            path.push_str(&format!(" > {}[{}]", a.kind, i));
            let d = a.diverge_at(b, path);
            path.truncate(saved);
            if d.is_some() {
                return d;
            }
        }
        None
    }

    /// Check the span invariants of spec `10_testing_corpus.md` §10.4 against
    /// the UTF-8 source: `start <= end`, `end <= source.len()`, both on char
    /// boundaries, children in source order and contained within the parent,
    /// and any leaf `value` text matching the span's source slice.
    pub fn validate_spans(&self, source: &str) -> Result<(), Divergence> {
        if self.span.start != 0 || self.span.end as usize != source.len() {
            return Err(diverge(
                self.kind.as_str(),
                format!(
                    "root span {:?} does not cover the full source of {} bytes",
                    self.span,
                    source.len()
                ),
            ));
        }
        self.validate_at(source, &mut self.kind.as_str().to_string())
    }

    fn validate_at(&self, source: &str, path: &mut String) -> Result<(), Divergence> {
        let (start, end) = (self.span.start as usize, self.span.end as usize);
        if start > end || end > source.len() {
            return Err(diverge(
                path,
                format!(
                    "span {:?} out of bounds for source of {} bytes",
                    self.span,
                    source.len()
                ),
            ));
        }
        if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            return Err(diverge(
                path,
                format!("span {:?} not on char boundaries", self.span),
            ));
        }
        if let Some(value) = &self.value {
            if &source[start..end] != value.text() {
                return Err(diverge(
                    path,
                    format!(
                        "value text {:?} != source slice {:?}",
                        value.text(),
                        &source[start..end]
                    ),
                ));
            }
        }
        let mut cursor = start;
        for (i, child) in self.children.iter().enumerate() {
            if (child.span.start as usize) < cursor || !self.span.contains(child.span) {
                return Err(diverge(
                    path,
                    format!(
                        "child {}[{}] span {:?} escapes parent span {:?} (or overlaps its predecessor)",
                        child.kind, i, child.span, self.span
                    ),
                ));
            }
            let saved = path.len();
            path.push_str(&format!(" > {}[{}]", child.kind, i));
            let r = child.validate_at(source, path);
            path.truncate(saved);
            r?;
            cursor = child.span.end as usize;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(kind: &str, start: u32, end: u32, value: NodeValue) -> CstNode {
        CstNode {
            kind: NodeKind::from(kind),
            span: Span::new(start, end),
            children: vec![],
            value: Some(value),
            flags: vec![],
        }
    }

    fn node(kind: &str, start: u32, end: u32, children: Vec<CstNode>) -> CstNode {
        CstNode {
            kind: NodeKind::from(kind),
            span: Span::new(start, end),
            children,
            value: None,
            flags: vec![],
        }
    }

    #[test]
    fn node_kind_string_roundtrip() {
        assert_eq!(NodeKind::from("SourceFile"), NodeKind::SourceFile);
        assert_eq!(NodeKind::SourceFile.as_str(), "SourceFile");
        let passthrough = NodeKind::from("Arguments");
        assert_eq!(passthrough, NodeKind::Other("Arguments".to_string()));
        assert_eq!(passthrough.as_str(), "Arguments");
        assert_eq!(
            DiagnosticCode::from("UNEXPECTED_TOKEN"),
            DiagnosticCode::UnexpectedToken
        );
    }

    #[test]
    fn document_json_matches_schema_shape() {
        let json = r#"{
            "version": 1,
            "root": {
                "kind": "SourceFile",
                "span": {"start": 0, "end": 5},
                "children": [
                    {"kind": "Identifier", "span": {"start": 0, "end": 5},
                     "children": [], "value": {"Identifier": "hello"}, "flags": []}
                ],
                "value": null,
                "flags": []
            },
            "diagnostics": [
                {"code": "UNEXPECTED_TOKEN", "severity": "error", "message": "m",
                 "span": {"start": 0, "end": 0}, "expected": [], "recovery": "InsertedToken"}
            ]
        }"#;
        let doc = CanonicalDocument::from_json(json).unwrap();
        assert_eq!(doc.version, CANONICAL_CST_VERSION);
        assert_eq!(doc.root.kind, NodeKind::SourceFile);
        assert_eq!(
            doc.root.children[0].value,
            Some(NodeValue::Identifier("hello".to_string()))
        );
        assert_eq!(doc.diagnostics[0].code, DiagnosticCode::UnexpectedToken);
        assert_eq!(doc.diagnostics[0].severity, Severity::Error);
        // Serialize/deserialize is an identity (spec §10.4).
        let re = CanonicalDocument::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(re, doc);
    }

    #[test]
    fn first_divergence_reports_path() {
        let a = node(
            "SourceFile",
            0,
            5,
            vec![leaf(
                "Identifier",
                0,
                5,
                NodeValue::Identifier("hello".into()),
            )],
        );
        let b = node(
            "SourceFile",
            0,
            5,
            vec![leaf("Operator", 0, 5, NodeValue::Operator("hello".into()))],
        );
        let d = a.first_divergence(&b).unwrap();
        assert_eq!(d.path, "SourceFile > Identifier[0]");
        assert!(d.detail.contains("expected `Identifier`"), "{}", d.detail);
        assert_eq!(a.first_divergence(&a.clone()), None);
    }

    #[test]
    fn validate_spans_rejects_escaping_child() {
        let escaping = leaf("Identifier", 1, 4, NodeValue::Identifier("bcd".into()));
        let bad_parent = node("Block", 0, 3, vec![escaping.clone()]);
        let bad = node("SourceFile", 0, 4, vec![bad_parent]);
        assert!(bad.validate_spans("abcd").is_err());
        let good = node("SourceFile", 0, 4, vec![escaping]);
        assert!(good.validate_spans("abcd").is_ok());
    }

    #[test]
    fn validate_spans_requires_root_to_cover_source() {
        let partial = node("SourceFile", 0, 3, vec![]);
        let shifted = node("SourceFile", 1, 4, vec![]);
        assert!(partial.validate_spans("abcd").is_err());
        assert!(shifted.validate_spans("abcd").is_err());
    }

    #[test]
    fn validate_spans_requires_char_boundaries() {
        // "α" is 2 bytes; span (0,1) splits it.
        let bad = leaf("Identifier", 0, 1, NodeValue::Identifier("α".into()));
        assert!(bad.validate_spans("α").is_err());
        let good = leaf("Identifier", 0, 2, NodeValue::Identifier("α".into()));
        assert!(good.validate_spans("α").is_ok());
    }

    #[test]
    fn validate_spans_rejects_value_text_mismatch() {
        let bad = leaf("Identifier", 0, 4, NodeValue::Identifier("abce".into()));
        assert!(bad.validate_spans("abcd").is_err());
    }
}
