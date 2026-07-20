//! Canonical Expr head registry for metaprogramming paths.
//!
//! Julia `Expr` heads cross three sjulia subsystems: quote construction,
//! macro-return lowering, and runtime `eval`. Keep the symbolic names and
//! per-path coverage in one table so unsupported directions are visible when a
//! new head is added.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use subset_julia_vm_bytecode::value::ExprValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExprHead {
    Escape,
    HygienicScope,
    Block,
    Function,
    Macro,
    Struct,
    Assign,
    Const,
    Export,
    Public,
    UsingStatement,
    AddAssign,
    Call,
    For,
    While,
    If,
    ElseIf,
    Let,
    Local,
    Global,
    String,
    TypeAssert,
    Where,
    Subtype,
    Tuple,
    Vect,
    Row,
    Hcat,
    Vcat,
    Ref,
    MacroCall,
    Generator,
    Comprehension,
    /// `Expr(:filter, cond, binding...)` — a generator's filtered binding
    /// group (Issue #10923).
    Filter,
    /// `Expr(:flatten, generator)` — the whitespace `for ... for ...`
    /// nested-generator form (Issue #10923).
    Flatten,
    Arrow,
    Curly,
    ParametrizedTypeExpression,
    Interpolation,
    Splat,
    Adjoint,
    Return,
    Quote,
    Dot,
    Try,
    SymbolicLabel,
    SymbolicGoto,
    Comparison,
    AndAnd,
    OrOr,
    CopyAst,
    Parameters,
    Kw,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExprHeadSpec {
    pub head: ExprHead,
    pub cst_to_expr_value: bool,
    pub macro_return_to_stmt: bool,
    pub macro_return_to_expr: bool,
    pub runtime_eval: bool,
    /// Issue #10627: whether the **static** stdlib/Base macro quote-expansion
    /// Pass-2 codegen dispatcher (`quote_constructor_to_code_with_hygiene`'s
    /// outer match, `lowering/expr/quote/code_generation.rs`) recognizes this
    /// head directly. This is `false` for some heads the static path DOES
    /// support (e.g. `Local`/`Assign`) because those are only recognized as a
    /// STATEMENT nested inside a `Block`/`If`/`Try`/`For`/`While`/`Let` body
    /// (handled by the per-statement matches in
    /// `lowering/expr/quote/handlers.rs`), never as the outer dispatch's own
    /// head — this column tracks only the outer dispatch, mirroring
    /// `macro_return_to_stmt`/`macro_return_to_expr` above for the dynamic
    /// path's own two dispatchers.
    pub static_quote_top_level: bool,
}

impl ExprHead {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Escape => "escape",
            Self::HygienicScope => "hygienic-scope",
            Self::Block => "block",
            Self::Function => "function",
            Self::Macro => "macro",
            Self::Struct => "struct",
            Self::Assign => "=",
            Self::Const => "const",
            Self::Export => "export",
            Self::Public => "public",
            Self::UsingStatement => "usingstatement",
            Self::AddAssign => "+=",
            Self::Call => "call",
            Self::For => "for",
            Self::While => "while",
            Self::If => "if",
            Self::ElseIf => "elseif",
            Self::Let => "let",
            Self::Local => "local",
            Self::Global => "global",
            Self::String => "string",
            Self::TypeAssert => "::",
            Self::Where => "where",
            Self::Subtype => "<:",
            Self::Tuple => "tuple",
            Self::Vect => "vect",
            Self::Row => "row",
            Self::Hcat => "hcat",
            Self::Vcat => "vcat",
            Self::Ref => "ref",
            Self::MacroCall => "macrocall",
            Self::Generator => "generator",
            Self::Comprehension => "comprehension",
            Self::Filter => "filter",
            Self::Flatten => "flatten",
            Self::Arrow => "->",
            Self::Curly => "curly",
            Self::ParametrizedTypeExpression => "parametrizedtypeexpression",
            Self::Interpolation => "$",
            Self::Splat => "...",
            Self::Adjoint => "'",
            Self::Return => "return",
            Self::Quote => "quote",
            Self::Dot => ".",
            Self::Try => "try",
            Self::SymbolicLabel => "symboliclabel",
            Self::SymbolicGoto => "symbolicgoto",
            Self::Comparison => "comparison",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::CopyAst => "copyast",
            Self::Parameters => "parameters",
            Self::Kw => "kw",
            Self::Meta => "meta",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "escape" => Self::Escape,
            "hygienic-scope" => Self::HygienicScope,
            "block" => Self::Block,
            "function" => Self::Function,
            "macro" => Self::Macro,
            "struct" => Self::Struct,
            "=" => Self::Assign,
            "const" => Self::Const,
            "export" => Self::Export,
            "public" => Self::Public,
            "usingstatement" => Self::UsingStatement,
            "+=" => Self::AddAssign,
            "call" => Self::Call,
            "for" => Self::For,
            "while" => Self::While,
            "if" => Self::If,
            "elseif" => Self::ElseIf,
            "let" => Self::Let,
            "local" => Self::Local,
            "global" => Self::Global,
            "string" => Self::String,
            "::" => Self::TypeAssert,
            "where" => Self::Where,
            "<:" => Self::Subtype,
            "tuple" => Self::Tuple,
            "vect" => Self::Vect,
            "row" => Self::Row,
            "hcat" => Self::Hcat,
            "vcat" => Self::Vcat,
            "ref" => Self::Ref,
            "macrocall" => Self::MacroCall,
            "generator" => Self::Generator,
            "comprehension" => Self::Comprehension,
            "filter" => Self::Filter,
            "flatten" => Self::Flatten,
            "->" => Self::Arrow,
            "curly" => Self::Curly,
            "parametrizedtypeexpression" => Self::ParametrizedTypeExpression,
            "$" => Self::Interpolation,
            "..." => Self::Splat,
            "'" => Self::Adjoint,
            "return" => Self::Return,
            "quote" => Self::Quote,
            "." => Self::Dot,
            "try" => Self::Try,
            "symboliclabel" => Self::SymbolicLabel,
            "symbolicgoto" => Self::SymbolicGoto,
            "comparison" => Self::Comparison,
            "&&" => Self::AndAnd,
            "||" => Self::OrOr,
            "copyast" => Self::CopyAst,
            "parameters" => Self::Parameters,
            "kw" => Self::Kw,
            "meta" => Self::Meta,
            _ => return None,
        })
    }

    pub fn from_expr(expr: &ExprValue) -> Option<Self> {
        Self::from_name(expr.head.as_str())
    }

    pub fn is_expr(expr: &ExprValue, head: Self) -> bool {
        Self::from_expr(expr) == Some(head)
    }

    pub fn spec(self) -> &'static ExprHeadSpec {
        // INTERNAL: `self` is already a valid `ExprHead` enum value (never raw
        // user text — see `from_name`/`from_expr` above), so a lookup miss can
        // only mean a new `ExprHead` variant was added without a matching
        // `EXPR_HEAD_REGISTRY` entry — an sjulia compiler bug, not something
        // any Julia source (however malformed) can trigger. Matches the
        // `vm/exec/mod.rs` SystemTime precedent in docs/vm/PANIC_FREE.md's
        // "Exceptions" section: module-level deny with one documented,
        // targeted allow. `expr_head_registry_covers_all_variants` below
        // exhaustively verifies the invariant this depends on.
        #[allow(clippy::expect_used)]
        EXPR_HEAD_REGISTRY
            .iter()
            .find(|spec| spec.head == self)
            .expect("all ExprHead variants must be present in EXPR_HEAD_REGISTRY")
    }
}

pub const EXPR_HEAD_REGISTRY: &[ExprHeadSpec] = &[
    spec(ExprHead::Escape, true, true, true, false, false),
    spec(ExprHead::HygienicScope, true, true, true, false, false),
    spec(ExprHead::Block, true, true, true, true, true),
    // `macro_return_to_expr = true` (Issue #10926): a macro-returned ANONYMOUS
    // function expression (`Expr(:function, Expr(:tuple, params...), body)`)
    // converts in value position by lifting a lambda (mirrors `Arrow`); the
    // named-signature form stays statement-only (`macro_return_to_stmt`).
    // `static_quote_top_level = true` (Issue #10916): the static Pass-2
    // dispatcher handles both forms (`handle_function_expr`).
    spec(ExprHead::Function, true, true, true, false, true),
    // A macro definition returned from a macro expansion lowers as a statement
    // (registers the macro), mirroring `Function`/`Struct`. `@doc(str, macro …)`
    // reaches statement conversion this way (Issue #9159).
    spec(ExprHead::Macro, true, true, false, false, false),
    spec(ExprHead::Struct, true, true, false, true, false),
    spec(ExprHead::Assign, true, true, true, true, false),
    spec(ExprHead::Const, true, true, true, false, false),
    spec(ExprHead::Export, true, true, true, false, false),
    spec(ExprHead::Public, true, true, true, false, false),
    spec(ExprHead::UsingStatement, true, false, false, true, false),
    spec(ExprHead::AddAssign, true, true, false, false, false),
    spec(ExprHead::Call, true, true, true, true, true),
    spec(ExprHead::For, true, true, true, false, true),
    spec(ExprHead::While, true, false, false, false, true),
    spec(ExprHead::If, true, true, true, true, true),
    spec(ExprHead::ElseIf, true, true, true, true, true),
    spec(ExprHead::Let, true, true, true, true, true),
    spec(ExprHead::Local, true, true, true, false, false),
    spec(ExprHead::Global, true, true, false, false, false),
    spec(ExprHead::String, true, false, true, true, false),
    spec(ExprHead::TypeAssert, true, false, true, false, false),
    // `static_quote_top_level = true` (Issue #10916): value-position `where`
    // types convert via `handle_where_expr` (TypeVar/UnionAll `let`, mirrors
    // the dynamic path's #7844 arm).
    spec(ExprHead::Where, true, false, true, false, true),
    spec(ExprHead::Subtype, true, false, true, false, false),
    spec(ExprHead::Tuple, true, false, true, true, true),
    spec(ExprHead::Vect, true, false, true, true, false),
    spec(ExprHead::Row, true, false, true, false, false),
    spec(ExprHead::Hcat, true, false, true, false, false),
    spec(ExprHead::Vcat, true, false, true, false, false),
    spec(ExprHead::Ref, true, false, true, true, false),
    spec(ExprHead::MacroCall, true, false, true, false, true),
    // `macro_return_to_expr = true` (Issue #10626): a macro-returned
    // `Expr(:generator, ...)` / `Expr(:comprehension, ...)` now converts to the
    // same `Expr::Generator` / `Expr::Comprehension` IR a non-quoted
    // generator/comprehension produces (single-binding, unfiltered form only —
    // see `generator_binding_from_generator_value` in macro_runtime.rs).
    // `static_quote_top_level = true` (Issue #10916): the static Pass-2
    // dispatcher produces the same IR (`handle_generator_expr` /
    // `handle_comprehension_expr`, same single-binding constraint).
    spec(ExprHead::Generator, true, false, true, false, true),
    spec(ExprHead::Comprehension, true, false, true, false, true),
    // Filter/Flatten (Issue #10923) only ever appear NESTED inside a
    // generator/comprehension Expr — they are consumed structurally by
    // `generator_parts_from_*` during that arm's conversion, never as
    // top-level macro-returned values, so every standalone capability is
    // false. `cst_to_expr_value = true`: the quote constructor builds them.
    spec(ExprHead::Filter, true, false, false, false, false),
    spec(ExprHead::Flatten, true, false, false, false, false),
    // `static_quote_top_level = true` (Issue #10617): arrow lambdas lift via
    // `handle_arrow_expr`, mirroring the dynamic `arrow_expr_from_values`.
    spec(ExprHead::Arrow, true, false, true, false, true),
    spec(ExprHead::Curly, true, false, true, true, false),
    spec(
        ExprHead::ParametrizedTypeExpression,
        false,
        false,
        false,
        true,
        false,
    ),
    spec(ExprHead::Interpolation, true, false, true, false, false),
    spec(ExprHead::Splat, true, false, true, false, false),
    spec(ExprHead::Adjoint, true, false, true, false, false),
    spec(ExprHead::Return, true, true, true, true, false),
    spec(ExprHead::Quote, true, false, true, true, false),
    spec(ExprHead::Dot, true, false, true, false, false),
    spec(ExprHead::Try, true, true, true, true, true),
    spec(ExprHead::SymbolicLabel, true, false, false, false, false),
    spec(ExprHead::SymbolicGoto, true, false, false, false, false),
    spec(ExprHead::Comparison, true, false, false, true, false),
    spec(ExprHead::AndAnd, true, false, false, true, false),
    spec(ExprHead::OrOr, true, false, false, true, false),
    spec(ExprHead::CopyAst, true, false, false, true, false),
    spec(ExprHead::Parameters, true, false, false, false, false),
    spec(ExprHead::Kw, true, false, false, false, false),
    spec(ExprHead::Meta, true, true, false, false, false),
];

const fn spec(
    head: ExprHead,
    cst_to_expr_value: bool,
    macro_return_to_stmt: bool,
    macro_return_to_expr: bool,
    runtime_eval: bool,
    static_quote_top_level: bool,
) -> ExprHeadSpec {
    ExprHeadSpec {
        head,
        cst_to_expr_value,
        macro_return_to_stmt,
        macro_return_to_expr,
        runtime_eval,
        static_quote_top_level,
    }
}

/// Issue #10627: canonical Pass-1 macro-quote **hygiene binding-introduction**
/// classification, consulted by BOTH the static stdlib/Base macro
/// quote-expansion hygiene collector (`collect_introduced_vars`,
/// `lowering/expr/quote/handlers.rs`) and the dynamic VM-backed
/// macro-runtime hygiene collector (`collect_quote_local_names`,
/// `macro_runtime.rs`). See `docs/vm/LOWERING.md` "Static Stdlib/Base Macro
/// Quote-Expansion Hygiene" / "Scope: static path vs. dynamic path" for the
/// decision-table history this codifies (previously duplicated as two
/// independently hand-maintained matches over the same heads).
///
/// Each engine still performs its own tree walk — the static path's
/// pre-execution constructor `Expr` and the dynamic path's post-execution
/// runtime `Value`/`ExprValue` trees are structurally incompatible (#10626
/// investigated and rejected literally sharing the walk), and each engine
/// still owns its own name-registration side effect (gensym-and-substitute
/// vs. collect-into-a-`HashSet`-then-rename). What used to be duplicated is
/// only the PER-HEAD DECISION of "does this head introduce a
/// hygiene-relevant binding, and in what shape" — that decision now has
/// exactly one source of truth; drift on these heads (e.g. someone adding a
/// binding-introducing head to one engine's match but not the other's) is
/// impossible by construction because both engines dispatch through this
/// same function.
///
/// Both engines consult this only for the roles they act on and treat every
/// other role (including one they don't act on today) identically to `None`
/// — i.e. "no direct binding here, just recurse into children generically".
/// This is why `FunctionName` — acted on only by the dynamic path, since the
/// static path's Pass-2 codegen has no `Expr(:function, ...)` support at all
/// (Issue #10916) — is safe to expose uniformly: the static path simply
/// never reaches a `Function` head to classify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteBindingRole {
    /// No binding introduced by this head itself; recurse into every child.
    /// Covers `Block`, `While`, and — perhaps counterintuitively — `For`/
    /// `Let` too: a `for`/`let` binding is itself an `Assign`-shaped child
    /// one level down, so it is classified (and registered) when recursion
    /// reaches THAT node, not by the `For`/`Let` head.
    None,
    /// `local x` / `local x = v`: every child names (or assigns) a local.
    LocalDecl,
    /// Plain assignment `x = v`: child 0 is the (possibly compound) target,
    /// child 1 is the value. The two engines are NOT fully symmetric on the
    /// "compound" case: the dynamic path recursively unwraps a `Tuple`/
    /// `TypeAssert` target and registers every bare name inside it; the
    /// static path only extracts a bare-`Symbol` target (a destructuring
    /// target silently registers nothing). Unreachable by any current
    /// stdlib/Base macro today, tracked by follow-up Issue #10980.
    Assign,
    /// `try`/`catch`: child 1 is the `catch` variable (or `false`).
    TryCatchVar,
    /// A function definition's own name (unwrapping a `where` wrapper).
    FunctionName,
}

pub fn quote_binding_role(head: ExprHead) -> QuoteBindingRole {
    match head {
        ExprHead::Local => QuoteBindingRole::LocalDecl,
        ExprHead::Assign => QuoteBindingRole::Assign,
        ExprHead::Try => QuoteBindingRole::TryCatchVar,
        ExprHead::Function => QuoteBindingRole::FunctionName,
        _ => QuoteBindingRole::None,
    }
}

/// Issue #10627: when the static path's Pass-2 codegen catch-all rejects a
/// head with "quote expansion for Expr(:X, ...) not yet supported", look up
/// the tracked follow-up Issue for a KNOWN gap (as opposed to a genuinely
/// unrecognized head) so the error message and any differential test can
/// reference it directly instead of the caller having to know the mapping.
pub fn tracked_static_quote_gap_issue(head: ExprHead) -> Option<u32> {
    // Issue #10916's four heads (Function/Where/Comprehension/Generator) plus
    // Arrow (#10617) gained static Pass-2 handlers, so no head is a TRACKED
    // gap today. Keep the lookup (and its call site in the Pass-2 catch-all)
    // so the next known gap can name its Issue in the error text again.
    let _ = head;
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        quote_binding_role, tracked_static_quote_gap_issue, ExprHead, QuoteBindingRole,
        EXPR_HEAD_REGISTRY,
    };

    #[test]
    fn expr_head_registry_round_trips_names() {
        for spec in EXPR_HEAD_REGISTRY {
            let name = spec.head.as_str();
            assert_eq!(ExprHead::from_name(name), Some(spec.head));
            assert_eq!(spec.head.spec().head, spec.head);
        }
    }

    #[test]
    fn expr_head_registry_has_unique_names() {
        let mut names = EXPR_HEAD_REGISTRY
            .iter()
            .map(|spec| spec.head.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), EXPR_HEAD_REGISTRY.len());
    }

    // ── Issue #10627: shared Pass-1 hygiene binding-role classifier ─────────

    #[test]
    fn quote_binding_role_local_and_assign_and_try_and_function() {
        assert_eq!(
            quote_binding_role(ExprHead::Local),
            QuoteBindingRole::LocalDecl
        );
        assert_eq!(
            quote_binding_role(ExprHead::Assign),
            QuoteBindingRole::Assign
        );
        assert_eq!(
            quote_binding_role(ExprHead::Try),
            QuoteBindingRole::TryCatchVar
        );
        assert_eq!(
            quote_binding_role(ExprHead::Function),
            QuoteBindingRole::FunctionName
        );
    }

    #[test]
    fn quote_binding_role_control_flow_heads_introduce_no_binding_of_their_own() {
        // For/Let/While/Block introduce no binding at their own head -- a
        // for/let binding is itself a nested `Assign`-shaped child, picked up
        // when recursion reaches that child, not by the For/Let head.
        for head in [
            ExprHead::For,
            ExprHead::Let,
            ExprHead::While,
            ExprHead::Block,
        ] {
            assert_eq!(quote_binding_role(head), QuoteBindingRole::None, "{head:?}");
        }
    }

    #[test]
    fn quote_binding_role_is_a_total_function_over_every_registered_head() {
        // Every ExprHead must classify to *some* role without panicking --
        // this is what makes the classifier safe to call unconditionally from
        // both engines' generic recursion fallback.
        for spec in EXPR_HEAD_REGISTRY {
            let _ = quote_binding_role(spec.head);
        }
    }

    #[test]
    fn tracked_static_quote_gap_issue_has_no_tracked_gaps_after_10916() {
        // Issue #10916's four heads (plus Arrow, #10617) all gained static
        // Pass-2 handlers, so NO head names a tracked gap Issue anymore.
        for spec in EXPR_HEAD_REGISTRY {
            assert_eq!(
                tracked_static_quote_gap_issue(spec.head),
                None,
                "{:?}",
                spec.head
            );
        }
    }

    #[test]
    fn static_quote_top_level_matches_the_documented_fifteen_heads() {
        let expected_true = [
            ExprHead::Call,
            ExprHead::Block,
            ExprHead::MacroCall,
            ExprHead::Tuple,
            ExprHead::Try,
            ExprHead::If,
            ExprHead::ElseIf,
            ExprHead::For,
            ExprHead::While,
            ExprHead::Let,
            // Issues #10916/#10617.
            ExprHead::Function,
            ExprHead::Where,
            ExprHead::Comprehension,
            ExprHead::Generator,
            ExprHead::Arrow,
        ];
        for spec in EXPR_HEAD_REGISTRY {
            let expected = expected_true.contains(&spec.head);
            assert_eq!(
                spec.static_quote_top_level, expected,
                "{:?} static_quote_top_level mismatch",
                spec.head
            );
        }
    }

    /// Exhaustively proves `spec()`'s "every variant is registered" invariant
    /// (Issue #10908 Phase 3 of #10869): `covers_variant` is compiler-checked
    /// exhaustive over every `ExprHead` variant (no `_ =>` catch-all), so
    /// adding a new variant without a matching arm here is a compile error,
    /// not a silently-passing gap. Combined with
    /// `expr_head_registry_round_trips_names` above (which already calls
    /// `.spec()` on every entry actually present in `EXPR_HEAD_REGISTRY`),
    /// a new variant either gets a registry entry (covered by that test) or
    /// this match fails to compile — there is no way to add a variant that
    /// silently skips both.
    #[test]
    fn expr_head_registry_covers_all_variants() {
        fn covers_variant(head: ExprHead) {
            match head {
                ExprHead::Escape
                | ExprHead::HygienicScope
                | ExprHead::Block
                | ExprHead::Function
                | ExprHead::Macro
                | ExprHead::Struct
                | ExprHead::Assign
                | ExprHead::Const
                | ExprHead::Export
                | ExprHead::Public
                | ExprHead::UsingStatement
                | ExprHead::AddAssign
                | ExprHead::Call
                | ExprHead::For
                | ExprHead::While
                | ExprHead::If
                | ExprHead::ElseIf
                | ExprHead::Let
                | ExprHead::Local
                | ExprHead::Global
                | ExprHead::String
                | ExprHead::TypeAssert
                | ExprHead::Where
                | ExprHead::Subtype
                | ExprHead::Tuple
                | ExprHead::Vect
                | ExprHead::Row
                | ExprHead::Hcat
                | ExprHead::Vcat
                | ExprHead::Ref
                | ExprHead::MacroCall
                | ExprHead::Generator
                | ExprHead::Filter
                | ExprHead::Flatten
                | ExprHead::Comprehension
                | ExprHead::Arrow
                | ExprHead::Curly
                | ExprHead::ParametrizedTypeExpression
                | ExprHead::Interpolation
                | ExprHead::Splat
                | ExprHead::Adjoint
                | ExprHead::Return
                | ExprHead::Quote
                | ExprHead::Dot
                | ExprHead::Try
                | ExprHead::SymbolicLabel
                | ExprHead::SymbolicGoto
                | ExprHead::Comparison
                | ExprHead::AndAnd
                | ExprHead::OrOr
                | ExprHead::CopyAst
                | ExprHead::Parameters
                | ExprHead::Kw
                | ExprHead::Meta => {}
            }
        }
        for spec in EXPR_HEAD_REGISTRY {
            covers_variant(spec.head);
        }
    }
}
