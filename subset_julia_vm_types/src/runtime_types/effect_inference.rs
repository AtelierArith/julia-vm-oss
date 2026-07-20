//! Effect inference for built-in operations and expressions.
//!
//! This module implements effect inference rules for built-in functions,
//! operators, and expression types.

use super::{EffectBit, Effects};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, UnaryOp};

/// Infer effects for a built-in operation by name and argument effects.
///
/// # Fixed effect name-hint table (audited 2026-07-02, Issue #8441)
///
/// This table is consulted only when no body-derived summary is available
/// for the callee name: `function_effects::compute_expr_effects` and
/// `infer_expr_effects_with_callees` prefer the whole-program fixpoint /
/// reflection-seeded summaries and fall back here, and `ssa_ir::opt` does
/// the same with its `_with_effects` map. Every remaining entry belongs to
/// one of two audited classes:
///
/// 1. **Rust-builtin / lowering-only names** — no pure Julia Base method
///    body exists for the name, so there is nothing for the body walker to
///    prove and the entry is the only source of effect information:
///    `%`, `===`, `!`, `&&`, `||` (operator spellings that normally lower
///    to `BinaryOp`/`UnaryOp` IR), `:` (lowers to `Expr::Range`),
///    `println`, `print`, `write`, `string`, `typeof`, `isa`, `sizeof`,
///    `getfield`, `rand`, `randn`, `randexp`.
///
/// 2. **Pure Julia names whose body derivation is still weaker than the
///    hint** — Base methods exist, but the name-level fixpoint proves less
///    than the entry for one or more of these reasons, so retiring the
///    entry today would only pessimize consumers:
///    * *name-keyed method-set merging*: the summary conservatively merges
///      every Base method sharing the name; large generic families
///      (`==` 49 methods, `isequal` 34, `convert` 81, `iterate` 94,
///      `length`/`size`/`eltype`/`getindex`/`collect`/`show`/... ) include
///      looping or dispatch-heavy bodies that taint `:terminates` and the
///      tri-state bits;
///    * *recursion pessimism*: the fixpoint starts from the conservative
///      seed and cannot recover optimistic cycles, so self/mutually
///      recursive families (`isnan`/`isinf`/`isfinite` via their Complex
///      component-wise methods, variadic `+`/`*` reductions) converge
///      below the hint;
///    * *Rust-builtin leaves*: bodies bottom out in un-summarized builtin
///      names (e.g. the `sqrt`/`log` numeric kernels), which the walker
///      classifies conservatively.
///
/// Entries whose summaries the body walker *can* prove were retired and are
/// locked by the
/// `retired_effect_hints_are_body_provable_over_base_issue_8441` trip-wire
/// plus upstream-parity fixtures (`effects_infer_effects_retired_*_8441`):
/// `!==`, `ifelse`, `tuple`. Retire further entries only through
/// that protocol — prove body-derivation is at least as precise via the
/// trip-wire, add a parity fixture where the surface is observable, then
/// delete the entry — and never delete an entry the walker cannot prove
/// (that includes every "builtin migration" move of a name from Rust to
/// pure Julia: migrating the implementation does not by itself make the
/// name-level summary provable; see Issue #2634 and
/// docs/vm/BUILTIN_REMOVAL.md Layer 5).
pub fn infer_builtin_effects(name: &str, arg_effects: &[Effects]) -> Effects {
    match name {
        // Pure arithmetic operations (class 2: recursion pessimism — the
        // variadic reduction methods recurse pairwise; plus name-keyed
        // merging over the array/matrix methods, which allocate).
        "+" | "-" | "*" | "/" | "^" | "//" => infer_arithmetic_effects(arg_effects),
        // class 2 (name-keyed merging) except "%", which is lowering-only
        // (class 1: `a % b` lowers to BinaryOp::Mod; no Base method).
        "div" | "rem" | "mod" | "%" => {
            merge_with_args(Effects::effect_free_may_throw(), arg_effects)
        }

        // Comparison operations (pure). class 2 (name-keyed merging) except
        // "===", which is class 1 (VM egal; `a === b` lowers to
        // BinaryOp::Egal, no Base method).
        //
        // Retired to body-derived inference (Issue #8441): "!==" — its Base
        // method body proves the same pure summary through
        // `propagation::infer_program_effects`. Keep `isless` here: the
        // VersionNumber method compares variable-length prerelease/build tuples,
        // which is pure but not currently proven by the body walker (Issue #9372).
        "==" | "!=" | "<" | "<=" | ">" | ">=" | "===" | "isequal" | "isless" => {
            Effects::pure_arithmetic()
        }

        // Boolean operations (pure). "&"/"|"/"xor" are class 2 (name-keyed
        // merging over the integer/BigInt/Bool method families); "!" is
        // class 1 (lowers to UnaryOp::Not, no Base method); "&&"/"||" are
        // class 1 short-circuit syntax (never function values — reachable
        // only from synthetic IR).
        "!" | "&&" | "||" | "&" | "|" | "xor" => Effects::pure_arithmetic(),

        // Bitwise operations (pure). class 2 (name-keyed merging).
        "<<" | ">>" | ">>>" => Effects::pure_arithmetic(),

        // Math functions (pure, but may throw for domain errors).
        // class 2: bodies bottom out in un-summarized Rust numeric kernels
        // ("sqrt"), merge multi-method families ("log" has 8 methods), or
        // inherit those weaknesses through nested calls ("log2"/"log10"
        // divide by `log`).
        "sqrt" | "log" | "log10" | "log2" => {
            merge_with_args(Effects::effect_free_may_throw(), arg_effects)
        }
        // class 2: name-keyed merging over the numeric method families;
        // "isnan"/"isinf"/"isfinite" additionally hit recursion pessimism
        // (their Complex methods recurse component-wise).
        "sin" | "cos" | "tan" | "exp" | "abs" | "sign" | "floor" | "ceil" | "round" | "trunc"
        | "isnan" | "isinf" | "isfinite" => Effects::pure_arithmetic(),

        // Array indexing (getindex) - may throw BoundsError. class 2
        // (49-method family).
        "getindex" => Effects::array_getindex(),

        // Array mutation (setindex!) - side effects. class 2.
        "setindex!" => Effects::array_setindex(),

        // Array construction: effect-free + nothrow but NOT consistent — each call
        // returns a fresh, independently-mutable array, so they must not be CSE'd
        // or hoisted into a shared allocation (Issue #7176). class 2
        // (allocation loops taint `:terminates` in the body summaries).
        "zeros" | "ones" | "fill" | "similar" | "copy" => Effects::allocating(),

        // Array properties (pure). class 2 (name-keyed merging over large
        // container families).
        "length" | "size" | "ndims" | "eltype" | "axes" | "eachindex" => Effects::pure_arithmetic(),

        // IO operations (side effects). "println"/"print"/"write" are
        // class 1 (Rust IO builtins); "show"/"display" are class 2.
        "println" | "print" | "show" | "display" | "write" => Effects::with_side_effects(),

        // String operations (pure). class 2 except "string" (class 1, Rust
        // builtin).
        "string" | "isempty" | "startswith" | "endswith" | "contains" | "replace" | "split"
        | "join" | "uppercase" | "lowercase" | "strip" | "lstrip" | "rstrip" => {
            Effects::pure_arithmetic()
        }

        // Core builtins classified by upstream semantic category, mirroring
        // `julia/Compiler/src/tfuncs.jl builtin_effects` (Issue #4274).
        //
        // Pure builtins (`_PURE_BUILTINS`) and the consistent + effect-free +
        // inaccessiblememonly builtins that are nothrow for well-typed concrete
        // arguments: consistent, effect-free, nothrow, total.
        // "typeof"/"isa"/"sizeof" are class 1 (Rust builtins);
        // "nfields"/"typeassert"/"convert"/"promote_type" are class 2.
        //
        // Retired to body-derived inference (Issue #8441): "ifelse" and
        // "tuple" — both are single pure Julia Base methods (essentials.jl /
        // tuple.jl) whose bodies prove the same total summary (trip-wire:
        // `retired_effect_hints_are_body_provable_over_base_issue_8441`).
        "typeof" | "isa" | "nfields" | "typeassert" | "sizeof" | "convert" | "promote_type" => {
            Effects::pure_arithmetic()
        }

        // Consistent + effect-free builtins that may throw (e.g. an invalid
        // field selector). Upstream taints only `:nothrow`; the inferred
        // exception type is `Any` (Issue #4274). "getfield" is class 1
        // (VM builtin); "fieldtype" is class 2.
        "getfield" | "fieldtype" => Effects::effect_free_may_throw(),

        // `fieldnames` is effect-free and pure for the reflected signatures.
        // class 2.
        "fieldnames" => Effects::pure_arithmetic(),

        // Mutating collection operations. class 2 (the Array methods also
        // fall back to VM builtins).
        "push!" | "pop!" | "append!" | "deleteat!" => Effects::array_setindex(),

        // Iteration operations (pure). class 2 ("iterate" merges 94
        // methods).
        "iterate" | "first" | "last" => Effects::pure_arithmetic(),

        // `collect` materializes a fresh mutable array — allocating, not
        // consistent. class 2.
        "collect" => Effects::allocating(),

        // Range operations (pure). class 2 except ":" (class 1: lowers to
        // Expr::Range, no Base method).
        "range" | ":" | "step" => Effects::pure_arithmetic(),

        // Random number generation (side effects, modifies RNG state).
        // class 1 (Rust RNG builtins).
        "rand" | "randn" | "randexp" => Effects::with_side_effects(),

        // Default: conservative (arbitrary effects). Unknown calls may throw
        // or perform arbitrary work even when their arguments are pure.
        _ => Effects::arbitrary(),
    }
}

/// Infer effects for arithmetic operations.
/// Arithmetic is pure unless arguments have side effects.
fn infer_arithmetic_effects(arg_effects: &[Effects]) -> Effects {
    if arg_effects.is_empty() {
        return Effects::pure_arithmetic();
    }

    // Merge all argument effects
    let mut result = Effects::pure_arithmetic();
    for arg_eff in arg_effects {
        result = result.merge(arg_eff);
    }
    result
}

/// Merge argument effects conservatively.
#[cfg(test)]
fn merge_arg_effects(arg_effects: &[Effects]) -> Effects {
    if arg_effects.is_empty() {
        return Effects::arbitrary();
    }

    let mut result = arg_effects[0];
    for arg_eff in &arg_effects[1..] {
        result = result.merge(arg_eff);
    }
    result
}

/// Merge a base effect with all argument effects conservatively.
fn merge_with_args(base: Effects, arg_effects: &[Effects]) -> Effects {
    arg_effects.iter().fold(base, |acc, arg| acc.merge(arg))
}

/// Infer effects for IR builtins (`Expr::Builtin`) by opcode.
fn infer_builtin_op_effects(op: &BuiltinOp, arg_effects: &[Effects]) -> Effects {
    let base = match op {
        // RNG/time, runtime-eval, and other stateful / non-deterministic
        // operations are effectful (`consistent = AlwaysFalse`,
        // `effect_free = AlwaysFalse`). `Gensym` returns a *different* unique
        // symbol every call (and bumps a global counter), so CSE-merging two
        // `gensym()` calls would hand back the same symbol and break macro
        // hygiene; `GeneratedEval` / `MacroExpand` run arbitrary macro/eval
        // machinery. (Issue #9323 — audited by
        // `builtin_op_effect_classification_is_audited_issue_9323`.)
        BuiltinOp::Rand
        | BuiltinOp::Randn
        | BuiltinOp::Seed
        | BuiltinOp::TimeNs
        | BuiltinOp::Eval
        | BuiltinOp::EvalFile
        | BuiltinOp::IncludeString
        | BuiltinOp::MacroExpand
        | BuiltinOp::MacroExpandBang
        | BuiltinOp::GeneratedEval
        | BuiltinOp::Gensym
        | BuiltinOp::TestRecord
        | BuiltinOp::TestRecordBroken
        | BuiltinOp::TestRecordError
        | BuiltinOp::TestSetBegin
        | BuiltinOp::TestSetEnd => Effects::with_side_effects(),

        // Mutating collection operations.
        BuiltinOp::Push
        | BuiltinOp::Pop
        | BuiltinOp::PushFirst
        | BuiltinOp::PopFirst
        | BuiltinOp::Insert
        | BuiltinOp::DeleteAt
        | BuiltinOp::DictDelete
        | BuiltinOp::DictMergeBang
        | BuiltinOp::DictEmpty
        | BuiltinOp::DictGetBang => Effects::array_setindex(),

        // Fresh mutable-object allocation. Each call returns an independent,
        // non-egal value backed by an `Rc<RefCell<..>>` (or containing one),
        // whose identity and independent mutation are observable, so two
        // textually-identical calls must NOT be merged by CSE or hoisted into a
        // shared value. Classifying them `allocating()` (consistent = AlwaysFalse)
        // keeps `is_pure()` false — the same class as the `zeros`/`ones`
        // name-table entries (Issue #7176) and the RNG constructors (Issue #9270).
        //
        //   * RNG engines: `MersenneTwister(seed)` / `Xoshiro(seed)` /
        //     `StableRNG(seed)` (`Rc<RefCell<..>>` streams — Issue #9270).
        //   * Arrays (`Value::Memory`, `Rc`-backed): `zeros` / `ones` / `reshape`
        //     / `collect` / `subtypes` / `_methods_by_ftype`.
        //   * `lu(A)` returns a tuple of freshly-allocated factor arrays.
        //   * `Ref(x)` is a mutable single-element cell (`Rc<RefCell<Value>>`,
        //     Issue #5130) — `r1 = Ref(0); r2 = Ref(0)` must be two cells.
        //   * `Expr(head, args...)` / `esc(x)` build a fresh `Expr` whose `.args`
        //     is a shared, mutable `Rc<RefCell<..>>` array.
        //   * `merge(d1, d2)` allocates a fresh `Dict` (kept for exhaustiveness;
        //     the codegen arm routes to Pure Julia today).
        //
        // Audited exhaustively by
        // `builtin_op_effect_classification_is_audited_issue_9323` (Issue #9323).
        BuiltinOp::MersenneTwisterRNG
        | BuiltinOp::XoshiroRNG
        | BuiltinOp::StableRNG
        | BuiltinOp::Zeros
        | BuiltinOp::Ones
        | BuiltinOp::Reshape
        | BuiltinOp::Collect
        | BuiltinOp::Subtypes
        | BuiltinOp::Methods
        | BuiltinOp::Lu
        | BuiltinOp::Ref
        | BuiltinOp::ExprNew
        | BuiltinOp::Esc
        | BuiltinOp::DictMerge => Effects::allocating(),

        // Conservative default for non-mutating builtins that return an
        // immutable value (scalars, `Bool`, `Symbol`, `QuoteNode`, types,
        // tuples/views, iteration state, reflection queries, …). These are
        // CSE-safe: same inputs yield an indistinguishable result with no fresh
        // mutable identity. Adding a new mutable-object-allocating variant here
        // is guarded against by the exhaustive audit test (Issue #9323).
        _ => Effects::pure_arithmetic(),
    };
    merge_with_args(base, arg_effects)
}

/// Infer effects for a binary operation.
pub fn infer_binary_op_effects(op: &BinaryOp, left: &Effects, right: &Effects) -> Effects {
    let merged = left.merge(right);

    match op {
        // Arithmetic operations - pure if operands are pure
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow => {
            Effects {
                consistent: merged.consistent,
                effect_free: merged.effect_free,
                nothrow: merged.nothrow,
                terminates: true,
                notaskstate: merged.notaskstate,
                inaccessiblememonly: merged.inaccessiblememonly,
                noub: merged.noub,
                nonoverlayed: merged.nonoverlayed,
                nortcall: merged.nortcall,
            }
        }

        BinaryOp::IntDiv | BinaryOp::Mod => {
            merge_with_args(Effects::effect_free_may_throw(), &[*left, *right])
        }

        // Comparison operations - pure
        BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::Lt
        | BinaryOp::Le
        | BinaryOp::Gt
        | BinaryOp::Ge
        | BinaryOp::Egal    // === (object identity)
        | BinaryOp::NotEgal // !== (not object identity)
        | BinaryOp::Subtype => Effects::pure_arithmetic(), // <: (subtype check)

        // Boolean operations - pure
        BinaryOp::And | BinaryOp::Or => merged,
    }
}

/// Infer effects for a unary operation.
pub fn infer_unary_op_effects(op: &UnaryOp, operand: &Effects) -> Effects {
    match op {
        // Arithmetic negation - pure
        UnaryOp::Neg | UnaryOp::Pos => *operand,

        // Boolean negation - pure
        UnaryOp::Not => *operand,
    }
}

/// Infer effects for an expression.
pub fn infer_expr_effects(expr: &Expr) -> Effects {
    infer_expr_effects_with_callees(expr, &|_| None)
}

/// Infer effects for an expression, consulting `callee_lookup` for call
/// targets before the curated builtin name table (Issue #8441).
///
/// `callee_lookup` supplies body-derived effect summaries (whole-program
/// fixpoint results or reflection callee seeds). A hit is trusted over the
/// name table because it was proven from the actual method bodies in scope;
/// a miss falls back to `infer_builtin_effects` exactly as before, so this
/// entry point is a strict precision refinement of `infer_expr_effects`.
pub fn infer_expr_effects_with_callees(
    expr: &Expr,
    callee_lookup: &dyn Fn(&str) -> Option<Effects>,
) -> Effects {
    let recurse = |expr: &Expr| infer_expr_effects_with_callees(expr, callee_lookup);
    match expr {
        // Literals are pure and total
        Expr::Literal(_, _) => Effects::total(),

        // Variable references are pure (assuming no global mutation tracking)
        Expr::Var { .. } => Effects::pure_arithmetic(),

        // Binary operations
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let left_eff = recurse(left);
            let right_eff = recurse(right);
            infer_binary_op_effects(op, &left_eff, &right_eff)
        }

        // Unary operations
        Expr::UnaryOp { op, operand, .. } => {
            let operand_eff = recurse(operand);
            infer_unary_op_effects(op, &operand_eff)
        }

        // Function calls: a body-derived callee summary wins over the name
        // table; the summary is then combined with the argument (and keyword
        // argument value) expression effects.
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            let mut arg_effects: Vec<Effects> = args.iter().map(&recurse).collect();
            arg_effects.extend(kwargs.iter().map(|(_, value)| recurse(value)));
            match callee_lookup(function) {
                Some(callee) => merge_with_args(callee, &arg_effects),
                None => infer_builtin_effects(function, &arg_effects),
            }
        }

        // Built-in calls
        Expr::Builtin { name, args, .. } => {
            let arg_effects: Vec<Effects> = args.iter().map(&recurse).collect();
            infer_builtin_op_effects(name, &arg_effects)
        }

        // Array literal - pure construction
        Expr::ArrayLiteral { elements, .. } => {
            let mut result = Effects::pure_arithmetic();
            for elem in elements {
                result = result.merge(&recurse(elem));
            }
            result
        }

        // Tuple literal - pure construction
        Expr::TupleLiteral { elements, .. } => {
            let mut result = Effects::pure_arithmetic();
            for elem in elements {
                result = result.merge(&recurse(elem));
            }
            result
        }

        // NamedTuple literal - pure construction
        Expr::NamedTupleLiteral { fields, .. } => {
            let mut result = Effects::pure_arithmetic();
            for (_, expr) in fields {
                result = result.merge(&recurse(expr));
            }
            result
        }

        // Range - pure construction
        Expr::Range {
            start, stop, step, ..
        } => {
            let mut result = recurse(start);
            result = result.merge(&recurse(stop));
            if let Some(step_expr) = step {
                result = result.merge(&recurse(step_expr));
            }
            result
        }

        // Let blocks - effects of the body
        Expr::LetBlock { .. } => {
            // Conservative: assume body may have arbitrary effects
            Effects::arbitrary()
        }

        // Function references - pure
        Expr::FunctionRef { .. } => Effects::pure_arithmetic(),

        // Index access - may throw BoundsError. `noub` is `Conditional`
        // (upstream `NOUB_IF_NOINBOUNDS`, Issue #9496; see
        // `Effects::array_getindex`'s doc comment): the default bytecode
        // always bounds-checks and throws `BoundsError` instead of exhibiting
        // UB, so this is not `AlwaysFalse`; UB is reachable only through the
        // compiler's own statically-proven-in-bounds fast path, so this is not
        // unconditionally `AlwaysTrue` either.
        Expr::Index { array, indices, .. } => {
            let mut result = recurse(array);
            for idx in indices {
                result = result.merge(&recurse(idx));
            }
            // Add bounds check effect
            Effects {
                consistent: result.consistent,
                effect_free: result.effect_free,
                nothrow: false, // May throw BoundsError
                terminates: result.terminates,
                notaskstate: result.notaskstate,
                inaccessiblememonly: result.inaccessiblememonly,
                noub: EffectBit::Conditional,
                nonoverlayed: result.nonoverlayed,
                nortcall: result.nortcall,
            }
        }

        // Field access - pure
        Expr::FieldAccess { object, .. } => recurse(object),

        // Comprehension - effects of the body
        Expr::Comprehension { body, .. } | Expr::MultiComprehension { body, .. } => {
            // Comprehension creates new array, so it's mostly pure
            // but body may have side effects
            recurse(body)
        }

        // Generator - conservative (may iterate with effects)
        Expr::Generator { .. } => Effects::arbitrary(),

        // Ternary operator - merge all branches
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            let mut result = recurse(condition);
            result = result.merge(&recurse(then_expr));
            result = result.merge(&recurse(else_expr));
            result
        }

        // String concatenation - pure
        Expr::StringConcat { parts, .. } => {
            let mut result = Effects::pure_arithmetic();
            for part in parts {
                result = result.merge(&recurse(part));
            }
            result
        }

        // Control-flow-as-expression (short-circuit `cond && return x`,
        // `cond && break`, `cond || continue`; the parser lowers these to a
        // `BinaryOp { And/Or, .., ReturnExpr/BreakExpr/ContinueExpr }`). A
        // `return`/`break`/`continue` is a single control transfer: it never
        // throws, always terminates, is effect-free and consistent. Its effect
        // is therefore just the effect of evaluating the returned value (nothing
        // for `break`/`continue`). Without these arms the transfer fell through
        // to `Effects::arbitrary()`, which poisoned every Base method that uses
        // the short-circuit form — most visibly the total-order `_float_isless`
        // helper behind `isless(::Float64, ..)`, weakening the whole-program
        // `isless` summary below its retired pure hint (Issue #9439 / #9344).
        // `break`/`continue` only appear inside loops, and the loop statements
        // already force `terminates: false`, so `total()` here never over-claims
        // termination for the enclosing construct.
        Expr::ReturnExpr { value, .. } => match value {
            Some(v) => recurse(v),
            None => Effects::total(),
        },
        Expr::BreakExpr { .. } | Expr::ContinueExpr { .. } => Effects::total(),

        // Assignment expression - side effect
        Expr::AssignExpr { value, .. } => {
            let value_eff = recurse(value);
            Effects {
                consistent: EffectBit::AlwaysFalse,
                effect_free: EffectBit::AlwaysFalse,
                nothrow: value_eff.nothrow,
                terminates: value_eff.terminates,
                notaskstate: value_eff.notaskstate,
                inaccessiblememonly: false, // Mutates variable
                noub: value_eff.noub,
                nonoverlayed: value_eff.nonoverlayed,
                nortcall: value_eff.nortcall,
            }
        }

        // Default: conservative
        _ => Effects::arbitrary(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::BuiltinOp;
    use subset_julia_vm_ir::Span;

    fn test_span() -> Span {
        Span::new(0, 0, 1, 1, 0, 0)
    }

    #[test]
    fn test_infer_builtin_arithmetic() {
        let effects = infer_builtin_effects("+", &[]);
        assert!(effects.is_pure());
        assert!(effects.is_foldable());
    }

    #[test]
    fn short_circuit_return_is_pure_not_arbitrary_issue_9439() {
        // `a != a && return false` (as in Base `_float_isless`) lowers to a
        // `BinaryOp { And, Ne(a, a), ReturnExpr(false) }`. `ReturnExpr` used to
        // fall through to `Effects::arbitrary()`, weakening every method that
        // uses the short-circuit form — including the total-order `isless`
        // helper whose retired pure hint the #8441 trip-wire locks (Issue #9439).
        let ret_false = Expr::ReturnExpr {
            value: Some(Box::new(Expr::Literal(
                crate::ir::core::Literal::Bool(false),
                test_span(),
            ))),
            span: test_span(),
        };
        // The bare `ReturnExpr` alone must be pure/total.
        assert!(
            infer_expr_effects(&ret_false).is_pure(),
            "ReturnExpr(literal) must be pure, not arbitrary"
        );

        let short_circuit = Expr::BinaryOp {
            op: BinaryOp::And,
            left: Box::new(Expr::BinaryOp {
                op: BinaryOp::Ne,
                left: Box::new(Expr::var("a", test_span())),
                right: Box::new(Expr::var("a", test_span())),
                span: test_span(),
            }),
            right: Box::new(ret_false),
            span: test_span(),
        };
        let effects = infer_expr_effects(&short_circuit);
        assert!(
            effects.is_pure() && effects.is_foldable(),
            "cond && return literal must stay pure/foldable: {effects:?}"
        );

        // `break` / `continue` in short-circuit position are pure control transfers.
        for cf in [
            Expr::BreakExpr { span: test_span() },
            Expr::ContinueExpr { span: test_span() },
        ] {
            assert!(
                infer_expr_effects(&cf).is_pure(),
                "control transfer must be pure: {cf:?}"
            );
        }
    }

    #[test]
    fn test_issue_4274_throwing_effect_free_numeric_builtins() {
        for name in ["div", "rem", "mod", "%", "sqrt", "log", "log10", "log2"] {
            let effects = infer_builtin_effects(name, &[]);
            assert!(effects.consistent.is_always_true(), "{name}");
            assert!(effects.effect_free.is_always_true(), "{name}");
            assert!(!effects.nothrow, "{name}");
        }
    }

    #[test]
    fn test_issue_4274_pure_core_builtins_are_total() {
        // Pure / consistent+effect-free+nothrow Core builtins infer to TOTAL,
        // matching upstream `builtin_effects` category composition.
        // "tuple" and "ifelse" were retired to body-derived inference
        // (Issue #8441) and are covered by
        // `retired_effect_hints_are_body_provable_over_base_issue_8441`.
        for name in ["typeof", "nfields", "isa", "typeassert", "sizeof"] {
            let effects = infer_builtin_effects(name, &[]);
            assert!(effects.consistent.is_always_true(), "{name} consistent");
            assert!(effects.effect_free.is_always_true(), "{name} effect_free");
            assert!(effects.nothrow, "{name} nothrow");
            assert!(effects.is_pure(), "{name} pure");
            assert!(effects.is_foldable(), "{name} foldable");
        }
    }

    #[test]
    fn test_issue_4274_consistent_throwing_core_builtins() {
        // `getfield` / `fieldtype` are consistent + effect-free but may throw,
        // so upstream taints only `:nothrow` (inferred exception type `Any`).
        for name in ["getfield", "fieldtype"] {
            let effects = infer_builtin_effects(name, &[]);
            assert!(effects.consistent.is_always_true(), "{name} consistent");
            assert!(effects.effect_free.is_always_true(), "{name} effect_free");
            assert!(!effects.nothrow, "{name} nothrow tainted");
            // Effect-free + may-throw is not pure but is still foldable-eligible
            // once the throw is ruled out (consistent + effect-free + terminates).
            assert!(!effects.is_pure(), "{name} not pure");
            assert!(effects.terminates, "{name} terminates");
        }
    }

    #[test]
    fn test_infer_builtin_println() {
        let effects = infer_builtin_effects("println", &[]);
        assert!(!effects.is_pure());
        assert!(!effects.is_foldable());
        assert!(effects.effect_free.is_always_false());
    }

    #[test]
    fn test_infer_builtin_getindex() {
        let effects = infer_builtin_effects("getindex", &[]);
        assert!(!effects.nothrow); // May throw BoundsError
        assert!(effects.consistent.is_always_true());
    }

    #[test]
    fn test_infer_builtin_setindex() {
        let effects = infer_builtin_effects("setindex!", &[]);
        assert!(!effects.is_pure());
        assert!(!effects.is_foldable());
        assert!(effects.effect_free.is_always_false());
    }

    #[test]
    fn test_infer_binary_op_add() {
        let left = Effects::pure_arithmetic();
        let right = Effects::pure_arithmetic();
        let effects = infer_binary_op_effects(&BinaryOp::Add, &left, &right);
        assert!(effects.is_pure());
    }

    #[test]
    fn test_issue_4274_throwing_effect_free_binary_ops() {
        let left = Effects::pure_arithmetic();
        let right = Effects::pure_arithmetic();
        for op in [BinaryOp::IntDiv, BinaryOp::Mod] {
            let effects = infer_binary_op_effects(&op, &left, &right);
            assert!(effects.consistent.is_always_true());
            assert!(effects.effect_free.is_always_true());
            assert!(!effects.nothrow);
        }
    }

    #[test]
    fn test_infer_unary_op_neg() {
        let operand = Effects::pure_arithmetic();
        let effects = infer_unary_op_effects(&UnaryOp::Neg, &operand);
        assert!(effects.is_pure());
    }

    #[test]
    fn test_merge_arg_effects() {
        let pure = Effects::pure_arithmetic();
        let side_effect = Effects::with_side_effects();
        let merged = merge_arg_effects(&[pure, side_effect]);
        assert!(!merged.is_pure());
    }

    #[test]
    fn test_expr_builtin_push_is_not_pure() {
        let expr = Expr::Builtin {
            name: BuiltinOp::Push,
            args: vec![],
            span: test_span(),
        };
        let effects = infer_expr_effects(&expr);
        assert!(!effects.is_pure());
        assert!(effects.effect_free.is_always_false());
    }

    #[test]
    fn test_expr_builtin_rand_is_not_pure() {
        let expr = Expr::Builtin {
            name: BuiltinOp::Rand,
            args: vec![],
            span: test_span(),
        };
        let effects = infer_expr_effects(&expr);
        assert!(!effects.is_pure());
        assert!(effects.effect_free.is_always_false());
    }

    /// CSE-safety classification of a `BuiltinOp` for the exhaustive audit
    /// (Issue #9323). The single property that governs whether the straight-line
    /// CSE / value-numbering pass may merge two textually-identical
    /// `Expr::Builtin` calls is `Effects::is_pure()` (see
    /// `compile::ir_opt`), so each op is exactly one of:
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OpEffectClass {
        /// Must classify NON-pure. The op has observable side effects, is
        /// non-deterministic, mutates its arguments, OR allocates a fresh mutable
        /// object (array / dict / `Ref` cell / RNG engine / `Expr` with shared
        /// `.args` / …) whose identity and independent mutation are observable.
        /// Merging two such calls aliases one object into two bindings — the
        /// exact #9270 (RNG) / #7176 (`zeros`/`ones`) / #5130 (`Ref`) bug class.
        MustNotBePure,
        /// Safe to classify pure. The op returns an immutable value (scalar,
        /// `Bool`, `Symbol`, `QuoteNode`, type, tuple/view, iteration state,
        /// reflection query, …) with no fresh mutable identity, so CSE-merging
        /// two identical calls is sound.
        PureOk,
    }

    /// EXHAUSTIVE, wildcard-free classification of every `BuiltinOp` variant.
    ///
    /// This is the "fail closed" guard for Issue #9323: adding a new `BuiltinOp`
    /// variant makes this `match` non-exhaustive, so the test build stops
    /// compiling until the author explicitly decides whether the new op allocates
    /// a mutable object / has side effects (`MustNotBePure`) or is a pure value
    /// computation (`PureOk`). A `MustNotBePure` decision then also forces a
    /// non-pure arm in `infer_builtin_op_effects` (otherwise the op still falls
    /// into the `_ => pure_arithmetic()` default and
    /// `builtin_op_effect_classification_is_audited_issue_9323` fails). NEVER add
    /// a `_ =>` arm here — that would re-open the silent-default hole #9270 was.
    fn expected_effect_class(op: &BuiltinOp) -> OpEffectClass {
        use OpEffectClass::{MustNotBePure, PureOk};
        match op {
            // ── Side effects / non-determinism / eval ──────────────────────
            BuiltinOp::Rand
            | BuiltinOp::Randn
            | BuiltinOp::Seed
            | BuiltinOp::TimeNs
            | BuiltinOp::Eval
            | BuiltinOp::EvalFile
            | BuiltinOp::IncludeString
            | BuiltinOp::MacroExpand
            | BuiltinOp::MacroExpandBang
            | BuiltinOp::GeneratedEval
            | BuiltinOp::Gensym
            | BuiltinOp::TestRecord
            | BuiltinOp::TestRecordBroken
            | BuiltinOp::TestRecordError
            | BuiltinOp::TestSetBegin
            | BuiltinOp::TestSetEnd => MustNotBePure,

            // ── Argument mutation ──────────────────────────────────────────
            BuiltinOp::Push
            | BuiltinOp::Pop
            | BuiltinOp::PushFirst
            | BuiltinOp::PopFirst
            | BuiltinOp::Insert
            | BuiltinOp::DeleteAt
            | BuiltinOp::DictDelete
            | BuiltinOp::DictMergeBang
            | BuiltinOp::DictEmpty
            | BuiltinOp::DictGetBang => MustNotBePure,

            // ── Fresh mutable-object allocation (Rc / Rc<RefCell>) ──────────
            BuiltinOp::MersenneTwisterRNG
            | BuiltinOp::XoshiroRNG
            | BuiltinOp::StableRNG
            | BuiltinOp::Zeros
            | BuiltinOp::Ones
            | BuiltinOp::Reshape
            | BuiltinOp::Collect
            | BuiltinOp::Subtypes
            | BuiltinOp::Methods
            | BuiltinOp::Lu
            | BuiltinOp::Ref
            | BuiltinOp::ExprNew
            | BuiltinOp::Esc
            | BuiltinOp::DictMerge => MustNotBePure,

            // ── Immutable value results (CSE-safe) ─────────────────────────
            BuiltinOp::Sqrt
            | BuiltinOp::IfElse
            | BuiltinOp::Zero
            | BuiltinOp::Length
            | BuiltinOp::Size
            | BuiltinOp::Ndims
            | BuiltinOp::Det
            | BuiltinOp::HasKey
            | BuiltinOp::DictGet
            | BuiltinOp::DictKeys
            | BuiltinOp::DictValues
            | BuiltinOp::DictPairs
            | BuiltinOp::DictGetkey
            | BuiltinOp::TupleFirst
            | BuiltinOp::TupleLast
            | BuiltinOp::TypeOf
            | BuiltinOp::Isa
            | BuiltinOp::Eltype
            | BuiltinOp::Keytype
            | BuiltinOp::Valtype
            | BuiltinOp::Sizeof
            | BuiltinOp::Isbitstype
            | BuiltinOp::Supertype
            | BuiltinOp::Typename
            | BuiltinOp::FunctionName
            | BuiltinOp::Objectid
            | BuiltinOp::Isunordered
            | BuiltinOp::In
            | BuiltinOp::Iterate
            | BuiltinOp::RangeStep
            | BuiltinOp::Generator
            | BuiltinOp::SymbolNew
            | BuiltinOp::LineNumberNodeNew
            | BuiltinOp::QuoteNodeNew
            | BuiltinOp::GlobalRefNew
            | BuiltinOp::SplatInterpolation
            | BuiltinOp::IsDefined
            | BuiltinOp::HasMethod => PureOk,
        }
    }

    /// Every `BuiltinOp` variant, listed once. The length is cross-checked
    /// against `strum::VariantNames` below so a newly-added variant that a author
    /// forgot to list here also fails the audit (belt-and-suspenders with the
    /// exhaustive `expected_effect_class` match).
    fn all_builtin_ops() -> Vec<BuiltinOp> {
        vec![
            BuiltinOp::Rand,
            BuiltinOp::Sqrt,
            BuiltinOp::IfElse,
            BuiltinOp::TimeNs,
            BuiltinOp::Zeros,
            BuiltinOp::Ones,
            BuiltinOp::Reshape,
            BuiltinOp::Length,
            BuiltinOp::Size,
            BuiltinOp::Ndims,
            BuiltinOp::Push,
            BuiltinOp::Pop,
            BuiltinOp::PushFirst,
            BuiltinOp::PopFirst,
            BuiltinOp::Insert,
            BuiltinOp::DeleteAt,
            BuiltinOp::Zero,
            BuiltinOp::Lu,
            BuiltinOp::Det,
            BuiltinOp::StableRNG,
            BuiltinOp::XoshiroRNG,
            BuiltinOp::Randn,
            BuiltinOp::TupleFirst,
            BuiltinOp::TupleLast,
            BuiltinOp::HasKey,
            BuiltinOp::DictGet,
            BuiltinOp::DictDelete,
            BuiltinOp::DictKeys,
            BuiltinOp::DictValues,
            BuiltinOp::DictPairs,
            BuiltinOp::DictMerge,
            BuiltinOp::DictGetBang,
            BuiltinOp::DictMergeBang,
            BuiltinOp::DictEmpty,
            BuiltinOp::DictGetkey,
            BuiltinOp::Ref,
            BuiltinOp::TypeOf,
            BuiltinOp::Isa,
            BuiltinOp::Eltype,
            BuiltinOp::Keytype,
            BuiltinOp::Valtype,
            BuiltinOp::Sizeof,
            BuiltinOp::Isbitstype,
            BuiltinOp::Supertype,
            BuiltinOp::Typename,
            BuiltinOp::FunctionName,
            BuiltinOp::Subtypes,
            BuiltinOp::Objectid,
            BuiltinOp::Isunordered,
            BuiltinOp::Methods,
            BuiltinOp::HasMethod,
            BuiltinOp::In,
            BuiltinOp::Seed,
            BuiltinOp::Iterate,
            BuiltinOp::RangeStep,
            BuiltinOp::Collect,
            BuiltinOp::Generator,
            BuiltinOp::SymbolNew,
            BuiltinOp::ExprNew,
            BuiltinOp::LineNumberNodeNew,
            BuiltinOp::QuoteNodeNew,
            BuiltinOp::GlobalRefNew,
            BuiltinOp::Gensym,
            BuiltinOp::Esc,
            BuiltinOp::Eval,
            BuiltinOp::MacroExpand,
            BuiltinOp::MacroExpandBang,
            BuiltinOp::IncludeString,
            BuiltinOp::EvalFile,
            BuiltinOp::SplatInterpolation,
            BuiltinOp::TestRecord,
            BuiltinOp::TestRecordBroken,
            BuiltinOp::TestRecordError,
            BuiltinOp::TestSetBegin,
            BuiltinOp::TestSetEnd,
            BuiltinOp::IsDefined,
            BuiltinOp::GeneratedEval,
            BuiltinOp::MersenneTwisterRNG,
        ]
    }

    /// Prevention mechanism for Issue #9323 (sibling risk of #9270): assert that
    /// EVERY `BuiltinOp` classified as allocating-a-mutable-object /
    /// side-effecting (`MustNotBePure`) is actually inferred non-pure by
    /// `infer_builtin_op_effects`, and every immutable-value op stays pure. A new
    /// mutable-object-allocating variant that silently falls into the
    /// `_ => pure_arithmetic()` default (the #9270 root cause) fails this test
    /// instead of shipping a CSE-aliasing miscompile.
    #[test]
    fn builtin_op_effect_classification_is_audited_issue_9323() {
        use strum::VariantNames;

        let all_ops = all_builtin_ops();

        // Fail closed on a forgotten variant: the exhaustive `expected_effect_class`
        // match already refuses to compile, and this catches a stale `all_ops`.
        assert_eq!(
            all_ops.len(),
            BuiltinOp::VARIANTS.len(),
            "all_builtin_ops() is out of sync with BuiltinOp ({} listed vs {} variants) — \
             list the new variant and classify it in expected_effect_class",
            all_ops.len(),
            BuiltinOp::VARIANTS.len(),
        );

        for op in all_ops {
            // `merge_with_args(base, &[])` returns `base` unchanged, so this
            // isolates the op's intrinsic effect classification.
            let effects = infer_builtin_op_effects(&op, &[]);
            match expected_effect_class(&op) {
                OpEffectClass::MustNotBePure => assert!(
                    !effects.is_pure(),
                    "{op:?} allocates a mutable object / has side effects but is classified \
                     pure — CSE would alias two identical calls (Issue #9323 / #9270). \
                     Add it to a non-pure arm in infer_builtin_op_effects."
                ),
                OpEffectClass::PureOk => assert!(
                    effects.is_pure(),
                    "{op:?} is classified pure in expected_effect_class but \
                     infer_builtin_op_effects reports it non-pure ({effects:?}). \
                     Reconcile the classification."
                ),
            }
        }
    }

    /// Focused regression: the mutable-object-allocating `BuiltinOp`s that lower
    /// directly from user syntax (`Ref`, `zeros`, `ones`, `collect`) must each be
    /// classified non-pure through the full `Expr::Builtin` → `infer_expr_effects`
    /// path, mirroring `rng_constructors_are_not_pure_issue_9270`. Guards the
    /// exact siblings found in the Issue #9323 audit.
    #[test]
    fn mutable_object_constructors_are_not_pure_issue_9323() {
        for op in [
            BuiltinOp::Ref,
            BuiltinOp::Zeros,
            BuiltinOp::Ones,
            BuiltinOp::Collect,
            BuiltinOp::ExprNew,
        ] {
            let ctor = Expr::Builtin {
                name: op,
                args: vec![],
                span: test_span(),
            };
            assert!(
                !infer_expr_effects(&ctor).is_pure(),
                "{op:?} must not be classified pure (would enable CSE aliasing)"
            );
        }
    }
}
