//! Main-scope type-name visibility for display (Issue #11365).
//!
//! Upstream Julia prints a type bare (`Point{Int64}`) when its leaf name is
//! reachable unqualified from Main (declared at top level, or `using`-imported)
//! and as the full path from the top (`Main.M.B`) otherwise. sjulia's
//! `using`-import emission stores a real Main-scope global binding (a
//! `Value::DataType` under the bare leaf name, rebuilt on the cache-restore
//! lane), so runtime global state is the visibility authority.
//!
//! This module keeps a thread-local mirror of those bindings: `Vm::run()`
//! seeds it from frame-0 state (covering REPL-persisted and restored
//! sessions), and the global-store choke points update it incrementally as
//! `using` statements execute mid-run. The free formatting functions — which
//! have no `&Vm` — consult it at render time, mirroring the
//! `set_struct_name_registry` pattern (Issue #9198 S4). Thread-local per the
//! single-threaded VM model.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::vm::value::Value;

thread_local! {
    /// Bare leaf name bound at Main scope -> normalized family head of the
    /// bound type (`"Point11365"` -> `"Geo11365.Point11365"`).
    static MAIN_VISIBLE_TYPE_BINDINGS: RefCell<HashMap<String, String>> =
        RefCell::new(HashMap::new());
    /// Root segments of module-qualified struct families declared by the
    /// current program (`"Geo11365"`). Only user roots get the `Main.` display
    /// prefix; Base/Core spellings keep their historical form.
    static USER_MODULE_ROOTS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Param-free family head with any `Main.` prefix removed:
/// `"Main.Geo.Point{Int64}"` -> `"Geo.Point"`.
fn normalized_family(name: &str) -> &str {
    let head = name.split('{').next().unwrap_or(name);
    head.strip_prefix("Main.").unwrap_or(head)
}

/// Clear both registries. Called at `Vm::run()` entry before re-seeding.
pub(crate) fn reset_main_scope_visibility() {
    MAIN_VISIBLE_TYPE_BINDINGS.with(|m| m.borrow_mut().clear());
    USER_MODULE_ROOTS.with(|r| r.borrow_mut().clear());
}

/// Record the root segments of the program's module-qualified struct families.
pub(crate) fn set_user_module_roots<I: IntoIterator<Item = String>>(roots: I) {
    USER_MODULE_ROOTS.with(|r| {
        let mut r = r.borrow_mut();
        r.extend(roots);
    });
}

/// Observe a Main-scope (frame 0) binding: a `DataType` bound under a bare
/// identifier makes that leaf visible; rebinding the name to anything else
/// revokes it. Names with owner qualifiers or compiler-internal `#` prefixes
/// never participate.
pub(crate) fn note_main_scope_binding(name: &str, value: &Value) {
    if name.is_empty() || name.contains('.') || name.starts_with('#') {
        return;
    }
    match value {
        Value::DataType(jt) => {
            let family = normalized_family(&jt.to_string()).to_string();
            MAIN_VISIBLE_TYPE_BINDINGS.with(|m| {
                m.borrow_mut().insert(name.to_string(), family);
            });
        }
        _ => {
            MAIN_VISIBLE_TYPE_BINDINGS.with(|m| {
                let mut m = m.borrow_mut();
                if !m.is_empty() {
                    m.remove(name);
                }
            });
        }
    }
}

/// Whether Main has `leaf` bound to exactly the type family `family`
/// (an unrelated same-leaf binding does not make a different family visible).
pub(crate) fn main_visible_type_leaf(leaf: &str, family: &str) -> bool {
    MAIN_VISIBLE_TYPE_BINDINGS.with(|m| {
        m.borrow()
            .get(leaf)
            .is_some_and(|bound| bound == normalized_family(family))
    })
}

/// Whether `root` is a module root declared by the current program.
pub(crate) fn is_user_module_root(root: &str) -> bool {
    USER_MODULE_ROOTS.with(|r| r.borrow().contains(root))
}
