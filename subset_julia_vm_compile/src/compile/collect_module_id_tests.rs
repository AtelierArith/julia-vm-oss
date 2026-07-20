//! `register_module_ids` tests (Issue #10988 Phase 2a).
//!
//! Split out of `collect.rs`'s own `mod tests` so the parent file stays under
//! the 2000-line structural-debt threshold
//! (`scripts/check_structural_debt_inventory.sh`), following the sibling
//! `*_tests.rs` module precedent established by `constructor_tests.rs` /
//! `cache_issue_10969_tests.rs`.

use std::collections::HashMap;

use crate::compile::collect::{collect_module_info, register_module_ids};
use crate::ir::core::{Block, Module, Stmt};
use crate::span::Span;

fn dummy_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

fn block(stmts: Vec<Stmt>) -> Block {
    Block {
        stmts,
        span: dummy_span(),
    }
}

fn empty_module(name: &str, submodules: Vec<Module>) -> Module {
    Module {
        name: name.to_string(),
        is_bare: false,
        is_package_origin: false,
        is_base_origin: false,
        functions: Vec::new(),
        structs: Vec::new(),
        abstract_types: Vec::new(),
        primitive_types: Vec::new(),
        type_aliases: Vec::new(),
        submodules,
        usings: Vec::new(),
        macros: Vec::new(),
        exports: Vec::new(),
        publics: Vec::new(),
        body: block(vec![]),
        span: dummy_span(),
    }
}

/// `register_module_ids` must register every path `collect_module_info`
/// itself inserts into `module_functions`, in a byte-identical set — the
/// duplicated qualification rule (Issue #10988 doc comment above
/// `register_module_ids`) must never drift from the original.
#[test]
fn register_module_ids_matches_collect_module_info_paths_issue_10988() {
    // A.Sub and B.Sub: same LOCAL submodule name under different parents,
    // the case ModuleId must keep distinct via the fully-qualified path.
    let a = empty_module("A", vec![empty_module("Sub", vec![])]);
    let b = empty_module("B", vec![empty_module("Sub", vec![])]);

    let mut module_functions = HashMap::new();
    let mut module_exports = HashMap::new();
    let mut module_constants = HashMap::new();
    for module in [&a, &b] {
        collect_module_info(
            module,
            "",
            &mut module_functions,
            &mut module_exports,
            &mut module_constants,
        );
    }
    let expected_paths: Vec<&String> = module_functions.keys().collect();

    let mut registry = subset_julia_vm_bytecode::ModuleInternTable::new();
    for module in [&a, &b] {
        register_module_ids(module, "", &mut registry);
    }

    // Every path `collect_module_info` registered is also known to the
    // registry (byte-identical path SET, per the doc comment's no-drift
    // contract), and "Main" (pre-interned, id 0) is the only extra entry the
    // registry carries beyond the module tree itself.
    for path in &expected_paths {
        assert!(
            registry.lookup(path).is_some(),
            "registry missing path {path:?} that collect_module_info registered"
        );
    }
    assert_eq!(registry.len(), expected_paths.len() + 1);

    // Distinctness: A.Sub and B.Sub never collide.
    assert_ne!(registry.lookup("A.Sub"), registry.lookup("B.Sub"));
    assert!(registry.lookup("A.Sub").is_some());
    assert!(registry.lookup("B.Sub").is_some());
}
