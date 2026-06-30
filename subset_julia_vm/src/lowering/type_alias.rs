//! User-defined type alias expansion (Issue #5055).
//!
//! Supports both non-parametric aliases (`const IntVec = Vector{Int}`) and
//! parametric aliases (`MyVec{T} = Vector{T}`). Upstream Julia desugars a
//! parametric alias to a `UnionAll`-valued binding (`MyVec = (Vector{T} where
//! T)`), and a use `MyVec{Int}` instantiates that `UnionAll`, yielding
//! `Vector{Int}`. Since the no-JIT static pipeline resolves type names through
//! string-keyed lookups (`JuliaType::from_name_or_struct`), we model the same
//! behaviour by *expanding* an alias use into its target type string at
//! lowering time:
//!
//! ```text
//! MyVec{T} = Vector{T}   registers ("MyVec", params=["T"], target="Vector{T}")
//! MyVec{Int}             expands to "Vector{Int}" (positional substitution)
//! MyVec                  expands to "Vector"      (bare alias -> bare target base)
//! ```
//!
//! Non-parametric aliases register with an empty `params` list and expand by a
//! plain name swap. Expansion is applied at the three type-name chokepoints:
//! parametric type expressions, parametric-type call heads (constructors), and
//! function parameter annotations.
//!
//! The registry is thread-local and scoped to a single program/module lowering
//! pass (single-threaded per compilation in the static pipeline). It is
//! populated by a pre-scan before statement lowering and cleared afterwards.

use std::cell::RefCell;
use std::collections::HashMap;

/// A registered alias: positional type parameter names and the target type
/// template string. For `MyVec{T} = Vector{T}` the params are `["T"]` and the
/// target template is `"Vector{T}"`. For `IntVec = Vector{Int}` the params are
/// empty and the target template is `"Vector{Int}"`.
#[derive(Debug, Clone)]
struct AliasEntry {
    params: Vec<String>,
    target: String,
}

thread_local! {
    static ALIASES: RefCell<HashMap<String, AliasEntry>> = RefCell::new(HashMap::new());
    /// Lexically-scoped type-parameter names that must NOT be treated as aliases
    /// during the active lowering scope, even if a same-named top-level binding
    /// registered one. Each entry is one nested scope (a function signature's
    /// `where {...}` parameter names); the union of all entries is excluded.
    /// Pushed/popped by [`ScopedExclusion`] around signature parsing so that a
    /// method's `where T` parameter shadows a global `T = Int64` alias inside the
    /// parameter annotations (Issue #7847), mirroring the struct-parameter
    /// exclusion of Issue #7840.
    static EXCLUDED_PARAMS: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

/// RAII guard that, for its lifetime, excludes a set of type-parameter names
/// from alias expansion in [`expand`]. Used around function-signature parsing so
/// a method's `where`-clause parameter names take precedence over a same-named
/// top-level type alias (Issue #7847).
pub struct ScopedExclusion {
    active: bool,
}

impl ScopedExclusion {
    /// Push `names` onto the scoped-exclusion stack. A no-op (returns an inert
    /// guard) when `names` is empty so the common no-`where` path is free.
    pub fn new(names: &[String]) -> Self {
        if names.is_empty() {
            return ScopedExclusion { active: false };
        }
        EXCLUDED_PARAMS.with(|e| e.borrow_mut().push(names.to_vec()));
        ScopedExclusion { active: true }
    }
}

impl Drop for ScopedExclusion {
    fn drop(&mut self) {
        if self.active {
            EXCLUDED_PARAMS.with(|e| {
                e.borrow_mut().pop();
            });
        }
    }
}

/// True when `name` is currently shadowed by a scoped type parameter.
fn is_scoped_excluded(name: &str) -> bool {
    EXCLUDED_PARAMS.with(|e| {
        e.borrow()
            .iter()
            .any(|frame| frame.iter().any(|n| n == name))
    })
}

fn alias_name_is_excluded(name: &str, excluded: &[String]) -> bool {
    let leaf = name.rsplit('.').next().unwrap_or(name);
    excluded.iter().any(|e| e == name || e == leaf)
        || is_scoped_excluded(name)
        || is_scoped_excluded(leaf)
}

fn resolve_alias_entry(name: &str, excluded: &[String]) -> Option<AliasEntry> {
    if alias_name_is_excluded(name, excluded) {
        return None;
    }

    ALIASES.with(|a| {
        let aliases = a.borrow();
        if let Some(entry) = aliases.get(name) {
            return Some(entry.clone());
        }

        let leaf = name.rsplit('.').next()?;
        if leaf == name || alias_name_is_excluded(leaf, excluded) {
            return None;
        }

        let mut unique: Option<AliasEntry> = None;
        for (alias, entry) in aliases.iter() {
            if alias.rsplit('.').next() != Some(leaf) {
                continue;
            }
            if unique.as_ref().is_some_and(|existing| {
                existing.params != entry.params || existing.target != entry.target
            }) {
                return None;
            }
            unique = Some(entry.clone());
        }
        unique
    })
}

/// Register a type alias for the current lowering pass.
pub fn register(name: &str, params: Vec<String>, target: &str) {
    ALIASES.with(|a| {
        a.borrow_mut().insert(
            name.to_string(),
            AliasEntry {
                params,
                target: target.to_string(),
            },
        );
    });
}

/// Clear all registered aliases. Used by tests and at the very start of a
/// top-level lowering pass.
pub fn clear() {
    ALIASES.with(|a| a.borrow_mut().clear());
}

/// An opaque snapshot of the alias table, captured before a (possibly nested)
/// lowering pass and restored afterwards. This keeps nested lowering — such as
/// the stdlib/module loading triggered by `using Test` — from destroying the
/// aliases registered by the enclosing program (Issue #5055).
pub struct AliasScope {
    saved: HashMap<String, AliasEntry>,
}

/// Snapshot the current alias table, returning a guard whose `restore` call (or
/// drop) reinstates the captured state. The table itself is left intact so the
/// caller can immediately register additional aliases on top of it.
pub fn snapshot() -> AliasScope {
    let saved = ALIASES.with(|a| a.borrow().clone());
    AliasScope { saved }
}

impl AliasScope {
    /// Restore the snapshotted alias table, discarding aliases registered since
    /// the snapshot was taken.
    pub fn restore(self) {
        ALIASES.with(|a| *a.borrow_mut() = self.saved);
    }
}

/// True when any aliases are registered (cheap early-out for the common case).
fn is_empty() -> bool {
    ALIASES.with(|a| a.borrow().is_empty())
}

/// Split a type name string into `(base, Some(args))` where `args` are the
/// top-level comma-separated parameters inside the outermost `{...}`. Returns
/// `(name, None)` for a bare name. Respects nesting so `Foo{Bar{T}, S}` yields
/// `["Bar{T}", "S"]`.
fn split_curly(name: &str) -> (&str, Option<Vec<String>>) {
    let Some(open) = name.find('{') else {
        return (name, None);
    };
    if !name.ends_with('}') {
        return (name, None);
    }
    let base = name[..open].trim();
    let inner = &name[open + 1..name.len() - 1];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, &b) in inner.as_bytes().iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                args.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    (base, Some(args))
}

/// Substitute positional parameters into a target template. For template
/// `"Vector{T}"` with params `["T"]` and args `["Int"]`, yields `"Vector{Int}"`.
/// Substitution is whole-token aware so `T` does not match inside `Tuple`.
fn substitute(template: &str, params: &[String], args: &[String]) -> String {
    if params.is_empty() {
        return template.to_string();
    }
    let mut result = template.to_string();
    for (param, arg) in params.iter().zip(args.iter()) {
        result = replace_token(&result, param, arg);
    }
    result
}

/// Replace whole-identifier occurrences of `from` with `to` in `s`.
fn replace_token(s: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(from_bytes) {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + from_bytes.len();
            let after_ok = after_idx >= bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                out.push_str(to);
                i = after_idx;
                continue;
            }
        }
        // Copy one UTF-8 character to avoid splitting multibyte sequences.
        let ch_len = utf8_len(bytes[i]);
        let end = (i + ch_len).min(s.len());
        out.push_str(&s[i..end]);
        i = end;
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'!'
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// Expand a type-name string by resolving any registered alias at its base.
/// Recursively expands the alias target and any aliases used in arguments, so
/// nested aliases (`Foo{Bar{Int}}`) resolve fully. Non-alias names are returned
/// unchanged. Bounded recursion guards against pathological self-referential
/// definitions.
pub fn expand(name: &str) -> String {
    if is_empty() {
        return name.to_string();
    }
    expand_bounded(name, 16, &[])
}

/// Like [`expand`], but treats every name in `excluded` as if it were *not* a
/// registered alias, leaving those tokens verbatim. This keeps a construct's own
/// lexically scoped type parameters (e.g. the `T` in `struct Wrap{T} <:
/// AbstractVector{T}`) from being frozen to the value of a same-named top-level
/// global/alias (`T = Int64`). Upstream Julia scopes a struct's type parameters
/// to the struct, so a global of the same name is irrelevant when lowering the
/// declared parent type (Issue #7840).
pub fn expand_excluding(name: &str, excluded: &[String]) -> String {
    // Even when no aliases are registered we keep the early-out cheap; the
    // exclusion set only matters when an alias would otherwise be substituted.
    if is_empty() {
        return name.to_string();
    }
    expand_bounded(name, 16, excluded)
}

fn expand_bounded(name: &str, fuel: usize, excluded: &[String]) -> String {
    if fuel == 0 {
        return name.to_string();
    }
    let (base, args) = split_curly(name);

    // First, recursively expand any aliases appearing in the arguments.
    let expanded_args: Option<Vec<String>> = args.map(|a| {
        a.iter()
            .map(|arg| expand_bounded(arg, fuel - 1, excluded))
            .collect()
    });

    // A name shadowed by an enclosing construct's own type parameter is not an
    // alias here, regardless of any same-named global binding. The exclusion may
    // be an explicit `excluded` argument (struct parameters, Issue #7840) or a
    // thread-local scoped exclusion pushed around signature parsing (method
    // `where` parameters, Issue #7847).
    let entry = resolve_alias_entry(base, excluded);
    let Some(entry) = entry else {
        // Not an alias: reassemble base with expanded args (args may have
        // contained aliases that were expanded above).
        return match expanded_args {
            Some(args) => format!("{}{{{}}}", base, args.join(", ")),
            None => base.to_string(),
        };
    };

    match &expanded_args {
        // Parametric use `Alias{A, B}`: positionally substitute into target.
        Some(args) if !entry.params.is_empty() => {
            let substituted = substitute(&entry.target, &entry.params, args);
            // The target may itself name another alias (`MyVec{T} = MyArr{T}`).
            expand_bounded(&substituted, fuel - 1, excluded)
        }
        // Bare alias use `Alias` for a parametric alias: in upstream this is the
        // UnionAll itself; the bare target base (`Vector`) is the closest static
        // analogue.
        None if !entry.params.is_empty() => {
            let (target_base, _) = split_curly(&entry.target);
            expand_bounded(target_base, fuel - 1, excluded)
        }
        // Non-parametric alias used with explicit args (rare/invalid): swap base.
        Some(args) => {
            let (target_base, target_args) = split_curly(&entry.target);
            match target_args {
                Some(_) => expand_bounded(&entry.target, fuel - 1, excluded),
                None => format!("{}{{{}}}", target_base, args.join(", ")),
            }
        }
        // Non-parametric alias used bare: expand to its (recursively resolved)
        // target.
        None => expand_bounded(&entry.target, fuel - 1, excluded),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_clean<T>(f: impl FnOnce() -> T) -> T {
        clear();
        let r = f();
        clear();
        r
    }

    #[test]
    fn parametric_alias_substitutes_positional() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            assert_eq!(expand("MyVec{Int}"), "Vector{Int}");
            assert_eq!(expand("MyVec{Float64}"), "Vector{Float64}");
        });
    }

    #[test]
    fn bare_parametric_alias_expands_to_base() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            assert_eq!(expand("MyVec"), "Vector");
        });
    }

    #[test]
    fn non_parametric_alias_swaps_name() {
        with_clean(|| {
            register("IntVec", vec![], "Vector{Int}");
            assert_eq!(expand("IntVec"), "Vector{Int}");
        });
    }

    #[test]
    fn token_substitution_does_not_match_substrings() {
        with_clean(|| {
            register("Pair2", vec!["T".to_string()], "Tuple{T, T}");
            // `T` must not corrupt `Tuple`.
            assert_eq!(expand("Pair2{Int}"), "Tuple{Int, Int}");
        });
    }

    #[test]
    fn multi_param_substitution() {
        with_clean(|| {
            register(
                "MyDict",
                vec!["K".to_string(), "V".to_string()],
                "Dict{K, V}",
            );
            assert_eq!(expand("MyDict{String, Int}"), "Dict{String, Int}");
        });
    }

    #[test]
    fn nested_alias_in_argument_expands() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            // Vector{MyVec{Int}} -> Vector{Vector{Int}}
            assert_eq!(expand("Vector{MyVec{Int}}"), "Vector{Vector{Int}}");
        });
    }

    #[test]
    fn alias_chain_resolves() {
        with_clean(|| {
            register("MyArr", vec!["T".to_string()], "Vector{T}");
            register("MyVec", vec!["T".to_string()], "MyArr{T}");
            assert_eq!(expand("MyVec{Int}"), "Vector{Int}");
        });
    }

    #[test]
    fn unknown_name_unchanged() {
        with_clean(|| {
            register("MyVec", vec!["T".to_string()], "Vector{T}");
            assert_eq!(expand("Foo{Bar}"), "Foo{Bar}");
            assert_eq!(expand("Int"), "Int");
        });
    }

    #[test]
    fn excluded_param_shadows_same_named_global_alias() {
        // Issue #7840: a top-level `T = Int64` registers a non-parametric alias
        // `T -> Int64`. When lowering a struct's parent `AbstractVector{T}`, the
        // struct's own param `T` must shadow that alias so the parametric
        // template is preserved instead of frozen to `AbstractVector{Int64}`.
        with_clean(|| {
            register("T", vec![], "Int64");
            assert_eq!(expand("AbstractVector{T}"), "AbstractVector{Int64}");
            assert_eq!(
                expand_excluding("AbstractVector{T}", &["T".to_string()]),
                "AbstractVector{T}"
            );
        });
    }

    #[test]
    fn excluded_does_not_block_unrelated_aliases() {
        // Excluding `T` must not stop a genuinely different alias from resolving.
        with_clean(|| {
            register("T", vec![], "Int64");
            register("MyVec", vec!["S".to_string()], "Vector{S}");
            assert_eq!(
                expand_excluding("MyVec{T}", &["T".to_string()]),
                "Vector{T}"
            );
            // Non-excluded alias still resolves even with a non-empty excl set.
            assert_eq!(
                expand_excluding("MyVec{Int}", &["T".to_string()]),
                "Vector{Int}"
            );
        });
    }

    #[test]
    fn qualified_alias_expands_by_unique_leaf_issue_8406() {
        // Module-qualified where bounds such as `T <: AbstractAlgebra.RingElement`
        // must resolve through the same alias entry as the unqualified
        // `RingElement` when the leaf is unique in the current lowering scope.
        with_clean(|| {
            register(
                "RingElement",
                vec![],
                "Union{RingElem, Integer, Rational, AbstractFloat}",
            );
            assert_eq!(
                expand("AbstractAlgebra.RingElement"),
                "Union{RingElem, Integer, Rational, AbstractFloat}"
            );
        });
    }

    #[test]
    fn qualified_alias_leaf_ambiguity_stays_unchanged_issue_8406() {
        with_clean(|| {
            register("A.RingElement", vec![], "Int64");
            register("B.RingElement", vec![], "Float64");
            assert_eq!(expand("C.RingElement"), "C.RingElement");
            assert_eq!(expand("A.RingElement"), "Int64");
        });
    }

    #[test]
    fn scoped_exclusion_shadows_alias_within_scope_only() {
        // Issue #7847: a method `where T` parameter must shadow a same-named
        // top-level alias (`T = Int64`) while its signature is parsed. The
        // `ScopedExclusion` guard makes the plain `expand` (used by the
        // signature parser) treat `T` as a type variable for its lifetime, then
        // restores normal alias resolution on drop.
        with_clean(|| {
            register("T", vec![], "Int64");
            // Outside any scope, the global alias wins (top-level `x::T` uses).
            assert_eq!(expand("Tuple{T, Int64}"), "Tuple{Int64, Int64}");
            {
                let _g = ScopedExclusion::new(&["T".to_string()]);
                // Inside the scope, `T` is left verbatim as a type variable.
                assert_eq!(expand("Tuple{T, Int64}"), "Tuple{T, Int64}");
                // An unrelated name is unaffected by the exclusion.
                assert_eq!(expand("Int64"), "Int64");
            }
            // After the guard drops, normal resolution resumes.
            assert_eq!(expand("Tuple{T, Int64}"), "Tuple{Int64, Int64}");
        });
    }

    #[test]
    fn scoped_exclusion_nests_and_pops() {
        // Nested scopes union their excluded names and each pops independently.
        with_clean(|| {
            register("T", vec![], "Int64");
            register("S", vec![], "Float64");
            {
                let _outer = ScopedExclusion::new(&["T".to_string()]);
                assert_eq!(expand("S"), "Float64"); // S not excluded yet
                {
                    let _inner = ScopedExclusion::new(&["S".to_string()]);
                    assert_eq!(expand("T"), "T");
                    assert_eq!(expand("S"), "S");
                }
                // Inner scope dropped: S resolves again, T still excluded.
                assert_eq!(expand("S"), "Float64");
                assert_eq!(expand("T"), "T");
            }
            assert_eq!(expand("T"), "Int64");
        });
    }

    #[test]
    fn scoped_exclusion_empty_is_inert() {
        // An empty name set must not push a scope (no effect on resolution).
        with_clean(|| {
            register("T", vec![], "Int64");
            let _g = ScopedExclusion::new(&[]);
            assert_eq!(expand("T"), "Int64");
        });
    }
}
