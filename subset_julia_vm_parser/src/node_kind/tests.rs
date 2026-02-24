//! Tests for node_kind module

use super::*;
use std::collections::HashSet;

/// Verify that `all_variants()` has no duplicates and its length
/// matches `variant_count()` (which is backed by an exhaustive match).
/// If a new NodeKind is added but not in `all_variants()`, the exhaustive
/// match in `variant_count()` will fail to compile. This test additionally
/// catches duplicate entries. (Issue #1627)
#[test]
fn test_all_variants_is_exhaustive() {
    let all = NodeKind::all_variants();
    let unique: HashSet<NodeKind> = all.iter().copied().collect();

    assert_eq!(
        all.len(),
        unique.len(),
        "all_variants() contains duplicates: {} total but {} unique",
        all.len(),
        unique.len()
    );

    // Cross-check with variant_count (compile-time exhaustive match)
    assert_eq!(
        all.len(),
        NodeKind::variant_count(),
        "all_variants().len() != variant_count() — lists are out of sync"
    );
}

/// Verify that every variant in `all_variants()` round-trips through as_str/from_str
#[test]
fn test_all_variants_roundtrip() {
    for &kind in NodeKind::all_variants() {
        let s = kind.as_str();
        let parsed: NodeKind = s.parse().unwrap();
        assert_eq!(
            kind, parsed,
            "Round-trip failed for {:?}: as_str()={:?}, parsed back as {:?}",
            kind, s, parsed
        );
    }
}

#[test]
fn test_from_str() {
    assert_eq!(
        "source_file".parse::<NodeKind>().unwrap(),
        NodeKind::SourceFile
    );
    assert_eq!(
        "function_definition".parse::<NodeKind>().unwrap(),
        NodeKind::FunctionDefinition
    );
    assert_eq!(
        "identifier".parse::<NodeKind>().unwrap(),
        NodeKind::Identifier
    );
    assert_eq!(
        "unknown_type".parse::<NodeKind>().unwrap(),
        NodeKind::Unknown
    );
}

#[test]
fn test_as_str() {
    assert_eq!(NodeKind::SourceFile.as_str(), "source_file");
    assert_eq!(NodeKind::FunctionDefinition.as_str(), "function_definition");
    assert_eq!(NodeKind::Identifier.as_str(), "identifier");
}

#[test]
fn test_roundtrip() {
    let kinds = [
        NodeKind::SourceFile,
        NodeKind::FunctionDefinition,
        NodeKind::IfStatement,
        NodeKind::BinaryExpression,
        NodeKind::Identifier,
    ];

    for kind in kinds {
        assert_eq!(kind.as_str().parse::<NodeKind>().unwrap(), kind);
    }
}
