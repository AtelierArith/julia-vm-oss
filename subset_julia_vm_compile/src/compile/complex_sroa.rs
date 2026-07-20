//! Two-field isbits-struct scalar-replacement-of-aggregates (SROA) — Issue #9198
//! S2/S3 (`compile::complex_sroa`; the name is historical — S2 shipped the
//! `Complex{Float64}`-only pass, S3 generalizes it to any 2-field `Float64`
//! immutable struct).
//!
//! Design record: `docs/vm/REGISTER_VM.md` §"Multi-Slot Scalar (isbits Immutable
//! Struct) Unboxing", Design A (slot-pair / SROA). A function local that is
//! *stably* a small isbits immutable struct is a group of scalar fields; upstream
//! Julia keeps such a value unboxed (register/stack expanded), so its arithmetic
//! is the pure `base/*.jl` at native speed with no per-operation heap box. sjulia
//! instead boxes every construction/result as a `StructInstance { Vec<Value> }`,
//! so a typed `z = z*z + c` loop (or `p = V2(p.x+1.0, p.y+2.0)`) allocates per
//! iteration.
//!
//! This pass performs that unboxing as a **source-to-source Core IR rewrite** (the
//! sjulia analogue of Julia's SROA): a proven-2-field-`Float64`-struct local `v`
//! is split into two `Float64` locals `re`/`im` (part 0 / part 1), and every
//! operation on it is decomposed into real `Float64` arithmetic / field reads on
//! those two locals. The rewritten IR then compiles through the *existing* typed
//! `Float64` machinery — slot typing (`slot.rs`/`slot_metadata.rs`) gives each
//! part an `f64` slot and the peephole (`peephole.rs`) fuses the loads/stores/
//! arith into the `Load{Mul,Add,Sub}F64Slot` family — so **no new instruction or
//! intrinsic** is added (Principle 3, Pure Julia First).
//!
//! ## What qualifies (Issue #9198 — S2 landed, S3 generalizes)
//!
//! Two families of 2-field isbits struct are recognized:
//!
//! * **`Complex{Float64}`** — the driving case. Supports the full arithmetic
//!   decomposition (`*`, `+`, `-`, `real`/`imag`/`abs2`/`z.re`/`z.im`, `conj`,
//!   `ComplexF64 ⊕ Real`, `Complex/Real` division), plus (S3) `im`-based literals
//!   whose coefficients are *provably `Float64`* (`0.0 + 0.0im`, `cr + ci*im` when
//!   `cr`/`ci` are provably `f64`) and (S3) a boxed `::Complex{Float64}` parameter
//!   used as a decomposed operand (its `re`/`im` are hoisted to `f64` locals at
//!   entry). `Complex{Float64}` is a parametric Base type whose fields are `::T`;
//!   its structural 2-`f64`-field layout cannot be read off a `StructDef` without
//!   instantiating `T=Float64`, so it stays recognized by constructor spelling —
//!   the *arithmetic* rules below are Complex semantics, not a dispatch shortcut.
//! * **User `struct T{x::Float64, y::Float64}`** (S3) — any *non-parametric,
//!   immutable, inner-constructor-free* user struct with exactly two concrete
//!   `Float64` fields, recognized *structurally* from its `StructDef` (no
//!   type-name special-casing, Principle 8/10). User structs have **no built-in
//!   arithmetic**, so only construction (`T(a, b)`), field reads (`p.x`/`p.y`),
//!   var copies, and materialization decompose; a user operator method call
//!   (`p + q`) is not inlined here, so it is left boxed (materialized).
//!
//! ## Correctness posture — sound classification + total rewrite + bail
//!
//! * **Soundness**: a local enters the SROA set only when *every* assignment to it
//!   provably yields the same 2-`f64`-field shape. For `Complex{Float64}`: the
//!   constructor forces `Float64`, `ComplexF64 ⊕ ComplexF64` stays `Float64`,
//!   `ComplexF64 ⊕ Real` promotes the real into `Float64`, and an `im`-literal is
//!   only decomposed when its coefficient is *provably `Float64`* (`2im` =
//!   `Complex{Int64}` is NOT decomposed — it stays boxed). For a user struct: only
//!   its own constructor / field reads / var copies decompose. Anything whose
//!   shape is not provable stays boxed.
//! * **Total rewrite**: once `v` is split, *every* syntactic occurrence of `v` is
//!   rewritten — decomposed or, in any other value position (escape: `push!`,
//!   `return v`, a call argument, string interpolation), **materialized** back to
//!   `T(v_re, v_im)`. Materialization is always valid, so escapes are correct by
//!   construction.
//! * **Bail**: any construct whose scoping this pass does not model soundly
//!   (`let`, `AssignExpr`, quotes, a comprehension/loop binding that shadows a
//!   split name, a nested closure capturing it) makes the whole function revert to
//!   its original boxed form — never a miscompile.
//!
//! The pass runs on the user segment only (user functions + module functions);
//! Base functions and `main` are untouched. AoT codegen uses a separate IR path
//! (`src/aot/`) that does not call this pass, so AoT is unaffected.

use std::collections::{HashMap, HashSet};

use crate::ir::core::{
    BinaryOp, Block, Expr, Function, Literal, Stmt, StructDef, TypedParam, UnaryOp,
};
use crate::span::Span;
use crate::types::JuliaType;

use super::ir_opt::UserSegmentOptimized;

/// Reserved name prefix for the generated part locals and staging temporaries. A
/// user local colliding with this prefix disables SROA for that function.
const CX_PREFIX: &str = "__sjulia_cx_";

fn re_name(v: &str) -> String {
    format!("{CX_PREFIX}re_{v}")
}

fn im_name(v: &str) -> String {
    format!("{CX_PREFIX}im_{v}")
}

fn zero_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

fn var(name: String) -> Expr {
    Expr::Var(name.into(), zero_span())
}

fn f64_lit(v: f64) -> Expr {
    Expr::Literal(Literal::Float(v), zero_span())
}

fn binop(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
        span: zero_span(),
    }
}

fn neg(e: Expr) -> Expr {
    Expr::UnaryOp {
        op: UnaryOp::Neg,
        operand: Box::new(e),
        span: zero_span(),
    }
}

fn field_access(object: String, field: &str) -> Expr {
    Expr::FieldAccess {
        object: Box::new(var(object)),
        field: field.to_string().into(),
        span: zero_span(),
    }
}

/// `T(re, im)` — the boxed materialization of a split local at an escape boundary.
fn materialize(ctor: &str, re: Expr, im: Expr) -> Expr {
    Expr::Call {
        function: ctor.to_string().into(),
        args: vec![re, im],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span: zero_span(),
    }
}

/// `Float64(e)` unless `e` is already a `Float64` literal (kept verbatim so
/// constant folding still sees it). Used to force a `Real` operand / constructor
/// argument into `f64` so the decomposed arithmetic stays typed. The struct field
/// type is `Float64`, so this matches the boxed constructor's `convert(Float64, …)`.
fn wrap_f64(e: Expr) -> Expr {
    match e {
        Expr::Literal(Literal::Float(_), _) => e,
        Expr::Literal(Literal::Int(v), _) => f64_lit(v as f64),
        other => Expr::Call {
            function: "Float64".to_string().into(),
            args: vec![other],
            kwargs: Vec::new(),
            splat_mask: vec![false],
            kwargs_splat_mask: Vec::new(),
            span: zero_span(),
        },
    }
}

/// The static `Complex{Float64}` shape (parametric Base type, recognized by
/// constructor spelling; fields `re`/`im`, arithmetic-capable).
fn complex_shape() -> Shape {
    Shape {
        ctor: "Complex{Float64}".to_string(),
        fields: ["re".to_string(), "im".to_string()],
        is_complex: true,
    }
}

/// A recognized 2-field `Float64` isbits-struct shape.
#[derive(Clone)]
struct Shape {
    /// Constructor / type name used to materialize (`Complex{Float64}`, `V2`, …).
    ctor: String,
    /// The two field names, in declaration order (`["re","im"]`, `["x","y"]`, …).
    fields: [String; 2],
    /// Whether this shape has the built-in `Complex` arithmetic decomposition.
    is_complex: bool,
}

/// Per-function SROA context: which names are split / decomposable and which
/// constructor names name a recognized 2-field shape.
struct Ctx {
    /// Split locals: name -> its shape. Assignments rewritten; escapes materialized.
    cf: HashMap<String, Shape>,
    /// Recognized shapes keyed by constructor / type name (Complex aliases + user
    /// structs). Used to recognize a constructor call as decomposable.
    shapes_by_ctor: HashMap<String, Shape>,
    /// `Complex{Float64}` parameters. Kept boxed (never split / materialized) but
    /// decomposable as an operand: their `re`/`im` are hoisted to `f64` locals at
    /// function entry, so `decompose(Var(p))` yields `(re_name(p), im_name(p))`.
    complex_params: HashSet<String>,
    /// Scalar-provability sets for this function (Issue #9654): names proven
    /// `Float64` / `Int64` on every binding, consulted by the `im`-literal
    /// coefficient and Real-operand rules.
    scalars: ScalarTypes,
}

impl Ctx {
    fn is_split(&self, v: &str) -> bool {
        self.cf.contains_key(v)
    }
    /// Whether `v` decomposes to `(re_name(v), im_name(v))` part locals.
    fn decomposable_var(&self, v: &str) -> bool {
        self.is_split(v) || self.complex_params.contains(v)
    }
    /// Whether `v` is (provably) a `Complex{Float64}` — participates in the
    /// arithmetic decomposition. A user-struct split var is decomposable but NOT
    /// complex (it has no arithmetic).
    fn is_complex_var(&self, v: &str) -> bool {
        self.complex_params.contains(v) || self.cf.get(v).is_some_and(|s| s.is_complex)
    }
    fn var_shape(&self, v: &str) -> Option<Shape> {
        if let Some(s) = self.cf.get(v) {
            Some(s.clone())
        } else if self.complex_params.contains(v) {
            Some(complex_shape())
        } else {
            None
        }
    }
}

/// Public entry: apply 2-field-f64-struct SROA to every user / module function in
/// the already-`ir_opt`-optimized user segment. `main` is intentionally skipped
/// (its locals are module-scope globals with REPL-persistence semantics).
pub(super) fn apply_to_user_segment(seg: &mut UserSegmentOptimized, structs: &[StructDef]) {
    let shapes_by_ctor = build_shapes(structs);
    for func in &mut seg.user_functions {
        sroa_function(func, &shapes_by_ctor);
    }
    for module in &mut seg.modules {
        for func in &mut module.functions {
            sroa_function(func, &shapes_by_ctor);
        }
    }
}

/// Build the recognized-shape table: `Complex{Float64}` (parametric Base type,
/// by spelling) plus every user struct that is *structurally* a 2-field `Float64`
/// immutable isbits struct (non-parametric, immutable, no inner constructor).
fn build_shapes(structs: &[StructDef]) -> HashMap<String, Shape> {
    let mut map = HashMap::new();
    // Complex{Float64} + its ComplexF64 alias spelling.
    map.insert("Complex{Float64}".to_string(), complex_shape());
    map.insert("ComplexF64".to_string(), complex_shape());
    for s in structs {
        if s.is_mutable || !s.type_params.is_empty() || !s.inner_constructors.is_empty() {
            // Mutable structs have identity/aliasing (StructRef); parametric structs
            // need instantiation; a custom inner constructor is not "store the args
            // verbatim". All three are unsound to split — skip.
            continue;
        }
        if s.fields.len() != 2 {
            continue;
        }
        if !s
            .fields
            .iter()
            .all(|f| matches!(f.as_julia_type(), Some(JuliaType::Float64)))
        {
            continue;
        }
        map.insert(
            s.name.clone(),
            Shape {
                ctor: s.name.clone(),
                fields: [s.fields[0].name.clone(), s.fields[1].name.clone()],
                is_complex: false,
            },
        );
    }
    map
}

/// Whether a parameter is annotated `::Complex{Float64}` (alias `::ComplexF64`
/// is expanded to `Complex{Float64}` at lowering).
fn is_complex_f64_param(p: &TypedParam) -> bool {
    if p.is_varargs {
        return false;
    }
    matches!(&p.type_annotation, Some(JuliaType::Struct(name)) if strip_ws(name) == "Complex{Float64}")
}

fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// A ComplexF64 arithmetic operand, decomposed into its real / imaginary parts
/// (both `f64`-typed expressions) or a plain real scalar.
enum Operand {
    Complex(Expr, Expr),
    Real(Expr),
}

/// Attempt SROA on one function in place. On any unsound construct the function is
/// left unchanged (boxed).
fn sroa_function(func: &mut Function, shapes_by_ctor: &HashMap<String, Shape>) {
    // A user local literally using the reserved prefix would collide with the
    // generated part/temp names — disable SROA for the whole function.
    if function_uses_reserved_prefix(func) {
        return;
    }

    let complex_params: HashSet<String> = func
        .params
        .iter()
        .filter(|p| is_complex_f64_param(p))
        .map(|p| p.name.clone())
        .collect();
    let params: HashSet<String> = func.params.iter().map(|p| p.name.clone()).collect();

    let mut analysis = Analysis::default();
    analysis.scan_block(&func.body);

    // Intra-function scalar provability (Issue #9654) — widens the im-literal
    // coefficient / Real-operand rules for both the split-local phases and the
    // value-position materialization.
    let scalars = compute_scalar_types(func, &analysis);

    // Whether the body mentions a complex source at all (`im`, a recognized
    // constructor, or a Complex{Float64} param). When there is no split local,
    // the total rewrite still runs iff this holds, so value-position complex
    // expressions (e.g. an `cr + ci*im` call argument) materialize to a direct
    // construction instead of a per-call dynamic dispatch (Issue #9654).
    let mentions_complex_source = !complex_params.is_empty() || {
        let mut referenced = HashSet::new();
        collect_referenced_names(&func.body, &mut referenced);
        referenced.contains("im") || shapes_by_ctor.keys().any(|k| referenced.contains(k))
    };

    // Candidate locals: assigned via plain `Assign`/`AddAssign`, never bound by a
    // loop/destructuring/catch/global form, never a parameter, never referenced
    // inside a nested closure body (capture would need a boxed cell).
    let candidates: Vec<String> = analysis
        .assigned
        .keys()
        .filter(|name| {
            !params.contains(*name)
                && !analysis.nonassign_bound.contains(*name)
                && !analysis.captured.contains(*name)
                && !analysis.declared_global.contains(*name)
        })
        .cloned()
        .collect();
    let cf = compute_split_locals(
        &candidates,
        &analysis,
        &complex_params,
        shapes_by_ctor,
        &scalars,
    );
    if cf.is_empty() && !mentions_complex_source {
        return;
    }

    let ctx = Ctx {
        cf,
        shapes_by_ctor: shapes_by_ctor.clone(),
        complex_params: complex_params.clone(),
        scalars,
    };

    // Total rewrite. On any unsound construct the rewrite bails (None) and the
    // function is left in its original boxed form.
    let mut counter: usize = 0;
    if let Some(mut new_body) = rewrite_block(&func.body, &ctx, &mut counter) {
        // Hoist the `re`/`im` parts of every referenced Complex{Float64} param to
        // `f64` locals at function entry (`__cx_re_p = p.re; __cx_im_p = p.im`) so
        // a loop reads f64 slots, not the boxed param, each iteration.
        let mut used = HashSet::new();
        for stmt in &new_body {
            collect_names_stmt(stmt, &mut used);
        }
        let mut hoist: Vec<Stmt> = Vec::new();
        for p in &ctx.complex_params {
            let (rn, imn) = (re_name(p), im_name(p));
            if used.contains(&rn) || used.contains(&imn) {
                hoist.push(Stmt::Assign {
                    var: rn,
                    value: field_access(p.clone(), "re"),
                    span: zero_span(),
                });
                hoist.push(Stmt::Assign {
                    var: imn,
                    value: field_access(p.clone(), "im"),
                    span: zero_span(),
                });
            }
        }
        if !hoist.is_empty() {
            hoist.extend(new_body);
            new_body = hoist;
        }
        func.body.stmts = new_body;
    }
}

/// Phases A/B + grounding: determine the split locals (name -> shape). Returns
/// an empty map when nothing qualifies (the caller may still run the total
/// rewrite for value-position materialization, Issue #9654).
fn compute_split_locals(
    candidates: &[String],
    analysis: &Analysis,
    complex_params: &HashSet<String>,
    shapes_by_ctor: &HashMap<String, Shape>,
    scalars: &ScalarTypes,
) -> HashMap<String, Shape> {
    if candidates.is_empty() {
        return HashMap::new();
    }

    // Phase A — determine each candidate's shape from a concrete source
    // (constructor / `im`-literal / `Complex{Float64}` param) propagated through
    // shaped vars. A candidate with conflicting shapes, or none, is excluded. This
    // subsumes grounding: an ungrounded cycle (`a=b; b=a`) never gets a shape.
    let mut shapes: HashMap<String, Shape> = HashMap::new();
    let mut conflicted: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for v in candidates {
            if conflicted.contains(v) {
                continue;
            }
            for rhs in &analysis.assigned[v] {
                if let Some(s) = infer_shape(rhs, &shapes, complex_params, shapes_by_ctor, scalars)
                {
                    match shapes.get(v) {
                        None => {
                            shapes.insert(v.clone(), s);
                            changed = true;
                        }
                        Some(existing) if existing.ctor != s.ctor => {
                            conflicted.insert(v.clone());
                            shapes.remove(v);
                            changed = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for v in &conflicted {
        shapes.remove(v);
    }
    if shapes.is_empty() {
        return HashMap::new();
    }

    // Phase B — greatest fixpoint: keep only locals whose *every* assignment RHS
    // provably decomposes under the current set. Removing a local can invalidate
    // another that referenced it, so iterate.
    let mut cf = shapes;
    loop {
        let ctx = Ctx {
            cf: cf.clone(),
            shapes_by_ctor: shapes_by_ctor.clone(),
            complex_params: complex_params.clone(),
            scalars: scalars.clone(),
        };
        let mut to_remove: Vec<String> = Vec::new();
        for v in cf.keys() {
            let rhss = &analysis.assigned[v];
            let ok = !rhss.is_empty() && rhss.iter().all(|rhs| decompose(rhs, &ctx).is_some());
            if !ok {
                to_remove.push(v.clone());
            }
        }
        if to_remove.is_empty() {
            break;
        }
        for v in to_remove {
            cf.remove(&v);
        }
        if cf.is_empty() {
            return HashMap::new();
        }
    }

    // Grounding: every kept local must have at least one assignment rooted in a
    // concrete source (a constructor / `im`-literal / complex param), reachable
    // through kept locals. Drops residual ungrounded self-referential cycles.
    let mut grounded: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for v in cf.keys() {
            if grounded.contains(v) {
                continue;
            }
            if analysis.assigned[v]
                .iter()
                .any(|rhs| rhs_is_grounded(rhs, &grounded, complex_params, shapes_by_ctor, scalars))
            {
                grounded.insert(v.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    cf.retain(|v, _| grounded.contains(v));
    cf
}

/// Infer the 2-field shape a candidate's assignment RHS is rooted in, propagating
/// through shaped vars. `None` = no concrete shape witness in this RHS.
fn infer_shape(
    e: &Expr,
    shapes: &HashMap<String, Shape>,
    complex_params: &HashSet<String>,
    shapes_by_ctor: &HashMap<String, Shape>,
    st: &ScalarTypes,
) -> Option<Shape> {
    if is_f64_im_pure(e, st) {
        return Some(complex_shape());
    }
    match e {
        Expr::Call { function, .. } if shapes_by_ctor.contains_key(function.as_str()) => {
            shapes_by_ctor.get(function.as_str()).cloned()
        }
        Expr::Var(w, _) => {
            if complex_params.contains(w.as_str()) {
                Some(complex_shape())
            } else {
                shapes.get(w.as_str()).cloned()
            }
        }
        Expr::Call { function, args, .. } if function == "conj" && args.len() == 1 => {
            infer_shape(&args[0], shapes, complex_params, shapes_by_ctor, st)
        }
        Expr::UnaryOp { operand, .. } => {
            infer_shape(operand, shapes, complex_params, shapes_by_ctor, st)
        }
        Expr::BinaryOp { left, right, .. } => {
            infer_shape(left, shapes, complex_params, shapes_by_ctor, st)
                .or_else(|| infer_shape(right, shapes, complex_params, shapes_by_ctor, st))
        }
        _ => None,
    }
}

/// Syntactic facts gathered from a function body for SROA eligibility.
#[derive(Default)]
struct Analysis {
    /// name -> list of assignment RHS expressions (`AddAssign` synthesized as
    /// `name + value`).
    assigned: HashMap<String, Vec<Expr>>,
    /// Names bound by a construct other than plain `Assign`/`AddAssign` (loop
    /// variables, destructuring targets, catch vars). Such names are excluded.
    nonassign_bound: HashSet<String>,
    /// Counted-`for` loop variables -> their `(start, end, step)` range
    /// expressions (Issue #9654). Feeds the scalar-provability lattice: a loop
    /// var over provably-integer bounds is a provably-integer scalar.
    for_ranges: HashMap<String, Vec<(Expr, Expr, Option<Expr>)>>,
    /// Names bound by an opaque binder (`for x in iterable`, tuple
    /// destructuring, catch vars) whose value type this pass cannot see.
    /// Excluded from the scalar-provability lattice (Issue #9654).
    opaque_bound: HashSet<String>,
    /// Names referenced inside any nested closure body (capture).
    captured: HashSet<String>,
    /// Names under a `global` declaration.
    declared_global: HashSet<String>,
}

impl Analysis {
    fn scan_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.scan_stmt(stmt);
        }
    }

    fn scan_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                self.assigned
                    .entry(var.clone())
                    .or_default()
                    .push(value.clone());
            }
            Stmt::AddAssign { var: v, value, .. } => {
                // `x += e` participates as `x = x + e`.
                let synth = binop(BinaryOp::Add, var(v.clone()), value.clone());
                self.assigned.entry(v.clone()).or_default().push(synth);
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                ..
            } => {
                self.nonassign_bound.insert(var.clone());
                self.for_ranges.entry(var.clone()).or_default().push((
                    start.clone(),
                    end.clone(),
                    step.clone(),
                ));
                self.scan_block(body);
            }
            Stmt::ForEach { var, body, .. } => {
                self.nonassign_bound.insert(var.clone());
                self.opaque_bound.insert(var.clone());
                self.scan_block(body);
            }
            Stmt::ForEachTuple { vars, body, .. } => {
                for v in vars {
                    self.nonassign_bound.insert(v.clone());
                    self.opaque_bound.insert(v.clone());
                }
                self.scan_block(body);
            }
            Stmt::While { body, .. } => self.scan_block(body),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.scan_block(then_branch);
                if let Some(eb) = else_branch {
                    self.scan_block(eb);
                }
            }
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                self.scan_block(try_block);
                if let Some(cv) = catch_var {
                    self.nonassign_bound.insert(cv.clone());
                    self.opaque_bound.insert(cv.clone());
                }
                for b in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    self.scan_block(b);
                }
            }
            Stmt::Block(block)
            | Stmt::Timed { body: block, .. }
            | Stmt::TestSet { body: block, .. } => {
                self.scan_block(block);
            }
            Stmt::DestructuringAssign { targets, .. } => {
                for t in targets {
                    self.nonassign_bound.insert(t.clone());
                    self.opaque_bound.insert(t.clone());
                }
            }
            Stmt::Global { names, .. } => {
                for n in names {
                    self.declared_global.insert(n.clone());
                }
            }
            Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
                // Any name referenced inside a nested closure body is treated as
                // captured (conservative — excludes it from SROA).
                collect_referenced_names(&func.body, &mut self.captured);
            }
            _ => {}
        }
    }
}

/// Whether the function contains a local whose name uses the reserved prefix.
fn function_uses_reserved_prefix(func: &Function) -> bool {
    if func.params.iter().any(|p| p.name.starts_with(CX_PREFIX)) {
        return true;
    }
    let mut names = HashSet::new();
    collect_referenced_names(&func.body, &mut names);
    names.iter().any(|n| n.starts_with(CX_PREFIX))
}

/// Collect every `Var` name (and assignment target) appearing in a block. Used
/// both for capture analysis and the reserved-prefix collision check.
fn collect_referenced_names(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_names_stmt(stmt, out);
    }
}

fn collect_names_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Assign { var, value, .. } | Stmt::AddAssign { var, value, .. } => {
            out.insert(var.to_string());
            collect_names_expr(value, out);
        }
        Stmt::For {
            var,
            start,
            end,
            step,
            body,
            ..
        } => {
            out.insert(var.to_string());
            collect_names_expr(start, out);
            collect_names_expr(end, out);
            if let Some(s) = step {
                collect_names_expr(s, out);
            }
            collect_referenced_names(body, out);
        }
        Stmt::ForEach {
            var,
            iterable,
            body,
            ..
        } => {
            out.insert(var.to_string());
            collect_names_expr(iterable, out);
            collect_referenced_names(body, out);
        }
        Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            ..
        } => {
            out.extend(vars.iter().cloned());
            collect_names_expr(iterable, out);
            collect_referenced_names(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_names_expr(condition, out);
            collect_referenced_names(body, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_names_expr(condition, out);
            collect_referenced_names(then_branch, out);
            if let Some(eb) = else_branch {
                collect_referenced_names(eb, out);
            }
        }
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_referenced_names(try_block, out);
            if let Some(cv) = catch_var {
                out.insert(cv.clone());
            }
            for b in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                collect_referenced_names(b, out);
            }
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            collect_referenced_names(block, out);
        }
        Stmt::Return { value: Some(e), .. } | Stmt::Expr { expr: e, .. } => {
            collect_names_expr(e, out)
        }
        Stmt::Test { condition, .. } => collect_names_expr(condition, out),
        Stmt::TestThrows { expr, .. } => collect_names_expr(expr, out),
        Stmt::IndexAssign {
            array,
            indices,
            value,
            ..
        } => {
            out.insert(array.clone());
            for i in indices {
                collect_names_expr(i, out);
            }
            collect_names_expr(value, out);
        }
        Stmt::FieldAssign { object, value, .. } => {
            out.insert(object.clone());
            collect_names_expr(value, out);
        }
        Stmt::DestructuringAssign { targets, value, .. } => {
            out.extend(targets.iter().cloned());
            collect_names_expr(value, out);
        }
        Stmt::DictAssign {
            dict, key, value, ..
        } => {
            out.insert(dict.clone());
            collect_names_expr(key, out);
            collect_names_expr(value, out);
        }
        Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
            collect_referenced_names(&func.body, out);
        }
        _ => {}
    }
}

fn collect_names_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Var(name, _) => {
            out.insert(name.to_string());
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_names_expr(left, out);
            collect_names_expr(right, out);
        }
        Expr::UnaryOp { operand, .. } | Expr::Convert { operand, .. } => {
            collect_names_expr(operand, out)
        }
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            out.insert(function.to_string());
            for a in args {
                collect_names_expr(a, out);
            }
            for (_, v) in kwargs {
                collect_names_expr(v, out);
            }
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            out.insert(function.to_string());
            for a in args {
                collect_names_expr(a, out);
            }
            for (_, v) in kwargs {
                collect_names_expr(v, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for a in args {
                collect_names_expr(a, out);
            }
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for e in elements {
                collect_names_expr(e, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_names_expr(array, out);
            for i in indices {
                collect_names_expr(i, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_names_expr(start, out);
            if let Some(s) = step {
                collect_names_expr(s, out);
            }
            collect_names_expr(stop, out);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_names_expr(body, out);
            collect_names_expr(iter, out);
            if let Some(f) = filter {
                collect_names_expr(f, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_names_expr(body, out);
            for (_, it) in iterations {
                collect_names_expr(it, out);
            }
            if let Some(f) = filter {
                collect_names_expr(f, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_names_expr(object, out),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, v) in fields {
                collect_names_expr(v, out);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_names_expr(key, out);
            collect_names_expr(value, out);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (k, v) in pairs {
                collect_names_expr(k, out);
                collect_names_expr(v, out);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for p in parts {
                collect_names_expr(p, out);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_names_expr(condition, out);
            collect_names_expr(then_expr, out);
            collect_names_expr(else_expr, out);
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(b) = base_expr {
                collect_names_expr(b, out);
            }
            for t in type_args {
                collect_names_expr(t, out);
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (n, v) in bindings {
                out.insert(n.to_string());
                collect_names_expr(v, out);
            }
            collect_referenced_names(body, out);
        }
        Expr::FunctionRef { name, .. } => {
            out.insert(name.to_string());
        }
        Expr::AssignExpr { var: v, value, .. } => {
            out.insert(v.to_string());
            collect_names_expr(value, out);
        }
        Expr::QuoteLiteral { constructor, .. } => collect_names_expr(constructor, out),
        Expr::ReturnExpr { value: Some(v), .. } => collect_names_expr(v, out),
        Expr::Literal(..)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::ReturnExpr { value: None, .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// ComplexF64-ness predicates (sound: `true` ⇒ definitely of that class)
// ---------------------------------------------------------------------------

/// `true` ⇒ `e` is definitely a `Complex` value participating in the arithmetic
/// decomposition. A user-struct split var is decomposable but NOT complex.
fn is_complex_expr(e: &Expr, ctx: &Ctx) -> bool {
    match e {
        Expr::Var(v, _) => ctx.is_complex_var(v) || v == "im",
        Expr::Call { function, args, .. } => {
            function.starts_with("Complex")
                || function == "complex"
                || (function == "conj" && args.len() == 1 && is_complex_expr(&args[0], ctx))
        }
        Expr::BinaryOp {
            op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div,
            left,
            right,
            ..
        } => is_complex_expr(left, ctx) || is_complex_expr(right, ctx),
        Expr::UnaryOp {
            op: UnaryOp::Neg | UnaryOp::Pos,
            operand,
            ..
        } => is_complex_expr(operand, ctx),
        _ => false,
    }
}

/// `true` ⇒ `e` is definitely a real (non-complex) scalar. Conservative: an
/// unknown `Var` (could be a complex parameter) is NOT definitely real, but a
/// scalar-provability-lattice member (Issue #9654) is.
fn is_definitely_real(e: &Expr, st: &ScalarTypes) -> bool {
    match e {
        Expr::Literal(lit, _) => matches!(
            lit,
            Literal::Int(_)
                | Literal::Int128(_)
                | Literal::Float(_)
                | Literal::Float32(_)
                | Literal::Float16(_)
                | Literal::Bool(_)
        ),
        Expr::Var(v, _) => st.f64s.contains(v.as_str()) || st.ints.contains(v.as_str()),
        Expr::Call { function, args, .. } => match function.as_str() {
            // These always return a Real regardless of argument type.
            "real" | "imag" | "abs2" | "abs" | "angle" | "hypot" | "Float64" | "Float32"
            | "Float16" => true,
            "convert" => args
                .first()
                .map(|t| matches!(t, Expr::Var(n, _) if is_real_type_name(n)))
                .unwrap_or(false),
            _ => false,
        },
        Expr::BinaryOp {
            op:
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::Pow,
            left,
            right,
            ..
        } => is_definitely_real(left, st) && is_definitely_real(right, st),
        Expr::UnaryOp {
            op: UnaryOp::Neg | UnaryOp::Pos,
            operand,
            ..
        } => is_definitely_real(operand, st),
        _ => false,
    }
}

fn is_real_type_name(name: &str) -> bool {
    matches!(
        name,
        "Float64" | "Float32" | "Float16" | "Int64" | "Int32" | "Int" | "Real" | "AbstractFloat"
    )
}

/// Whether a decomposition part is a bare `Var`/literal (cheap to duplicate for
/// `abs2`).
fn is_simple(e: &Expr) -> bool {
    matches!(e, Expr::Var(_, _) | Expr::Literal(_, _))
}

/// Intra-function scalar provability sets (Issue #9654): names proven to hold a
/// `Float64` / `Int64` scalar on *every* binding (typed params, counted-loop
/// vars over provably-integer bounds, locals whose every assignment RHS is
/// provable). Widens the `im`-literal coefficient and Real-operand rules so
/// value expressions like `cr + ci*im` (with `cr`/`ci` computed from typed
/// ints) decompose instead of staying a per-call dynamic dispatch.
#[derive(Default, Clone)]
struct ScalarTypes {
    f64s: HashSet<String>,
    ints: HashSet<String>,
}

/// Compute the scalar-provability sets by greatest fixpoint: seed every
/// eligible name optimistically, then remove any whose bindings are not all
/// provable under the current sets (handles self-referential accumulators like
/// `s = s + 1.0`). A name bound by an opaque binder / captured by a closure /
/// declared global never enters.
fn compute_scalar_types(func: &Function, analysis: &Analysis) -> ScalarTypes {
    let params: HashSet<&str> = func.params.iter().map(|p| p.name.as_str()).collect();
    let excluded = |name: &str| {
        analysis.captured.contains(name)
            || analysis.declared_global.contains(name)
            || analysis.opaque_bound.contains(name)
    };

    let mut st = ScalarTypes::default();
    for p in &func.params {
        if p.is_varargs || excluded(&p.name) {
            continue;
        }
        match &p.type_annotation {
            Some(JuliaType::Float64) => {
                st.f64s.insert(p.name.clone());
            }
            Some(JuliaType::Int64) => {
                st.ints.insert(p.name.clone());
            }
            _ => {}
        }
    }
    // Optimistic seeds: assigned locals (both sets) and counted-loop vars (ints).
    for v in analysis.assigned.keys() {
        if !params.contains(v.as_str()) && !excluded(v) && !analysis.for_ranges.contains_key(v) {
            st.f64s.insert(v.clone());
            st.ints.insert(v.clone());
        }
    }
    for v in analysis.for_ranges.keys() {
        if !params.contains(v.as_str()) && !excluded(v) {
            st.ints.insert(v.clone());
        }
    }

    loop {
        let mut changed = false;
        let check = st.clone();
        st.f64s.retain(|v| {
            let keep = params.contains(v.as_str())
                || analysis.assigned[v]
                    .iter()
                    .all(|rhs| provably_f64(rhs, &check));
            if !keep {
                changed = true;
            }
            keep
        });
        st.ints.retain(|v| {
            let keep = params.contains(v.as_str()) || {
                let assigns_ok = analysis
                    .assigned
                    .get(v)
                    .is_none_or(|rhss| rhss.iter().all(|rhs| provably_int(rhs, &check)));
                let ranges_ok = analysis.for_ranges.get(v).is_none_or(|ranges| {
                    ranges.iter().all(|(start, end, step)| {
                        provably_int(start, &check)
                            && provably_int(end, &check)
                            && step.as_ref().is_none_or(|s| provably_int(s, &check))
                    })
                });
                assigns_ok && ranges_ok
            };
            if !keep {
                changed = true;
            }
            keep
        });
        if !changed {
            break;
        }
    }
    st
}

/// `true` ⇒ `e` is provably a `Float64` scalar value (so decomposing an
/// `im`-literal whose coefficient is `e` is sound: the result is
/// `Complex{Float64}`, not `Complex{Int}`). A mixed `Float64 ⊗ Int64` operand
/// pair is `Float64` for `+ - * /`, and `Int64 / Int64` is `Float64` — the
/// upstream promotion rules for these concrete scalar types.
fn provably_f64(e: &Expr, st: &ScalarTypes) -> bool {
    match e {
        Expr::Literal(Literal::Float(_), _) => true,
        Expr::Var(v, _) => st.f64s.contains(v.as_str()),
        Expr::Call { function, args, .. } if function == "Float64" && args.len() == 1 => true,
        Expr::UnaryOp {
            op: UnaryOp::Neg | UnaryOp::Pos,
            operand,
            ..
        } => provably_f64(operand, st),
        Expr::BinaryOp {
            op: op @ (BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div),
            left,
            right,
            ..
        } => {
            let (lf, rf) = (provably_f64(left, st), provably_f64(right, st));
            if matches!(op, BinaryOp::Div) {
                // Float64/Int64 division of any operand mix is Float64.
                (lf || provably_int(left, st)) && (rf || provably_int(right, st))
            } else {
                (lf && (rf || provably_int(right, st))) || (rf && provably_int(left, st))
            }
        }
        _ => false,
    }
}

/// `true` ⇒ `e` is provably an `Int64` scalar value.
fn provably_int(e: &Expr, st: &ScalarTypes) -> bool {
    match e {
        Expr::Literal(Literal::Int(_), _) => true,
        Expr::Var(v, _) => st.ints.contains(v.as_str()),
        Expr::UnaryOp {
            op: UnaryOp::Neg | UnaryOp::Pos,
            operand,
            ..
        } => provably_int(operand, st),
        Expr::BinaryOp {
            op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Mod,
            left,
            right,
            ..
        } => provably_int(left, st) && provably_int(right, st),
        _ => false,
    }
}

fn is_im_ref(e: &Expr) -> bool {
    matches!(e, Expr::Var(v, _) if v == "im")
}

/// Whether `e` is a pure-imaginary `Float64` literal `k*im` / `im*k` with a
/// *provably-`Float64`* coefficient `k`. `im` = `Complex{Bool}(false,true)`, so
/// `k*im` is `Complex{Float64}(0.0, k)` **iff** `k` forces `Float64` (`2im` is
/// `Complex{Int64}` and is rejected here).
fn is_f64_im_pure(e: &Expr, st: &ScalarTypes) -> bool {
    matches!(
        e,
        Expr::BinaryOp { op: BinaryOp::Mul, left, right, .. }
            if (is_im_ref(right) && provably_f64(left, st))
                || (is_im_ref(left) && provably_f64(right, st))
    )
}

// ---------------------------------------------------------------------------
// Decomposition: ComplexF64 / 2-field-struct expr -> (re, im) real f64 exprs
// ---------------------------------------------------------------------------

/// Decompose a 2-field-`Float64` expression into `(re, im)` real `f64`
/// expressions with every inner decomposable var rewritten to its part locals.
/// Returns `None` when `e` is not a provable decomposable expression this pass
/// handles.
fn decompose(e: &Expr, ctx: &Ctx) -> Option<(Expr, Expr)> {
    match e {
        Expr::Var(v, _) if ctx.decomposable_var(v) => Some((var(re_name(v)), var(im_name(v)))),
        // Constructor of a recognized 2-field shape (Complex{Float64}, user struct).
        Expr::Call { function, args, .. } if ctx.shapes_by_ctor.contains_key(function.as_str()) => {
            let shape = ctx.shapes_by_ctor[function.as_str()].clone();
            decompose_ctor(&shape, args, ctx)
        }
        // Complex-only arithmetic (self-gated: a non-complex operand bails).
        Expr::Call { function, args, .. }
            if function == "conj" && args.len() == 1 && is_complex_expr(&args[0], ctx) =>
        {
            let (r, i) = decompose(&args[0], ctx)?;
            Some((r, neg(i)))
        }
        Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand,
            ..
        } if is_complex_expr(operand, ctx) => {
            let (r, i) = decompose(operand, ctx)?;
            Some((neg(r), neg(i)))
        }
        Expr::UnaryOp {
            op: UnaryOp::Pos,
            operand,
            ..
        } if is_complex_expr(operand, ctx) => decompose(operand, ctx),
        _ => {
            // `k*im` pure-imaginary Float64 literal ⇒ (0.0, k).
            if is_f64_im_pure(e, &ctx.scalars) {
                if let Expr::BinaryOp { left, right, .. } = e {
                    let coef = if is_im_ref(right) { left } else { right };
                    let rewritten = rewrite_real(coef, ctx)?;
                    // Skip the `Float64(…)` wrap when the coefficient is already
                    // provably `Float64` (identity convert) — one fewer builtin
                    // call in the decomposed hot path (Issue #9654).
                    let part = if provably_f64(coef, &ctx.scalars) {
                        rewritten
                    } else {
                        wrap_f64(rewritten)
                    };
                    return Some((f64_lit(0.0), part));
                }
            }
            if let Expr::BinaryOp {
                op, left, right, ..
            } = e
            {
                return decompose_binop(op, left, right, ctx);
            }
            None
        }
    }
}

/// Decompose a constructor call of a recognized 2-field shape.
fn decompose_ctor(shape: &Shape, args: &[Expr], ctx: &Ctx) -> Option<(Expr, Expr)> {
    match args.len() {
        2 => {
            // Both args are real scalars (the fields are Float64). Reject a complex
            // arg (would be a type error boxed too).
            if is_complex_expr(&args[0], ctx) || is_complex_expr(&args[1], ctx) {
                return None;
            }
            Some((
                coerce_ctor_arg(&args[0], ctx)?,
                coerce_ctor_arg(&args[1], ctx)?,
            ))
        }
        1 if shape.is_complex => {
            // `Complex{Float64}(a)` == `Complex{Float64}(a, 0.0)`.
            if is_complex_expr(&args[0], ctx) {
                return None;
            }
            Some((coerce_ctor_arg(&args[0], ctx)?, f64_lit(0.0)))
        }
        _ => None,
    }
}

/// Rewrite a constructor argument to the `Float64` value stored in the field's
/// part slot. The boxed constructor `convert(Float64, arg)`s each arg; when `arg`
/// is *already* provably `Float64` the convert is a no-op, so we skip the
/// `wrap_f64` (`Float64(…)`) call and store the value directly (avoids a redundant
/// per-iteration builtin call). An `Int`/unknown arg still gets `wrap_f64`.
fn coerce_ctor_arg(arg: &Expr, ctx: &Ctx) -> Option<Expr> {
    let rewritten = rewrite_real(arg, ctx)?;
    if arg_is_f64(arg, ctx) {
        Some(rewritten)
    } else {
        Some(wrap_f64(rewritten))
    }
}

/// Whether the (pre-rewrite) constructor arg already evaluates to a `Float64`
/// value, so the `convert(Float64, …)` is a no-op. Sound: only the clearly-f64
/// forms return `true`; a bare `Var` / `Int` literal (which could need widening)
/// returns `false`.
fn arg_is_f64(e: &Expr, ctx: &Ctx) -> bool {
    match e {
        Expr::Literal(Literal::Float(_), _) => true,
        Expr::Call { function, args, .. } if function == "Float64" && args.len() == 1 => true,
        // A field read of a decomposable 2-`Float64`-field struct is `Float64`.
        Expr::FieldAccess { object, field, .. } => decomposed_shape(object, ctx)
            .map(|s| s.fields.iter().any(|f| f == field))
            .unwrap_or(false),
        Expr::UnaryOp {
            op: UnaryOp::Neg | UnaryOp::Pos,
            operand,
            ..
        } => arg_is_f64(operand, ctx),
        Expr::BinaryOp {
            op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div,
            left,
            right,
            ..
        } => arg_is_f64(left, ctx) && arg_is_f64(right, ctx),
        _ => false,
    }
}

/// Classify a `+`/`-`/`*`/`/` operand. `None` bails the whole decomposition
/// (an unknown operand whose class we cannot determine). A `Real` operand is
/// returned pre-coerced to `f64` (the `Float64(…)` wrap is skipped when the
/// operand is already provably `Float64` — identity convert, Issue #9654).
fn classify(e: &Expr, ctx: &Ctx) -> Option<Operand> {
    // A user-struct split var is decomposable but not complex — it must NOT enter
    // the arithmetic combine (its `+` is a user method, not the Complex formula).
    if is_complex_expr(e, ctx) {
        if let Some((re, im)) = decompose(e, ctx) {
            return Some(Operand::Complex(re, im));
        }
        // Complex but not decomposable (e.g. `Complex{Int}`): bail.
        return None;
    }
    if is_definitely_real(e, &ctx.scalars) {
        let rewritten = rewrite_real(e, ctx)?;
        let coerced = if provably_f64(e, &ctx.scalars) {
            rewritten
        } else {
            wrap_f64(rewritten)
        };
        return Some(Operand::Real(coerced));
    }
    None
}

fn decompose_binop(op: &BinaryOp, left: &Expr, right: &Expr, ctx: &Ctx) -> Option<(Expr, Expr)> {
    let l = classify(left, ctx)?;
    let r = classify(right, ctx)?;
    match op {
        BinaryOp::Add => combine_addsub(l, r, false),
        BinaryOp::Sub => combine_addsub(l, r, true),
        BinaryOp::Mul => combine_mul(l, r),
        BinaryOp::Div => combine_div(l, r),
        _ => None,
    }
}

fn combine_addsub(l: Operand, r: Operand, sub: bool) -> Option<(Expr, Expr)> {
    let op = if sub { BinaryOp::Sub } else { BinaryOp::Add };
    match (l, r) {
        (Operand::Complex(lr, li), Operand::Complex(rr, ri)) => {
            Some((binop(op, lr, rr), binop(op, li, ri)))
        }
        (Operand::Complex(lr, li), Operand::Real(s)) => {
            // (lr ± s) + (li)i ; imaginary part unchanged (real added to real part)
            Some((binop(op, lr, s), li))
        }
        (Operand::Real(s), Operand::Complex(rr, ri)) => {
            let re = binop(op, s, rr);
            let im = if sub { neg(ri) } else { ri };
            Some((re, im))
        }
        // real ± real is not a complex result.
        (Operand::Real(_), Operand::Real(_)) => None,
    }
}

fn combine_mul(l: Operand, r: Operand) -> Option<(Expr, Expr)> {
    match (l, r) {
        (Operand::Complex(lr, li), Operand::Complex(rr, ri)) => {
            // (lr*rr - li*ri, lr*ri + li*rr) — the exact naive formula upstream
            // `*(::Complex, ::Complex)` uses (same op order, so bit-identical).
            let re = binop(
                BinaryOp::Sub,
                binop(BinaryOp::Mul, lr.clone(), rr.clone()),
                binop(BinaryOp::Mul, li.clone(), ri.clone()),
            );
            let im = binop(
                BinaryOp::Add,
                binop(BinaryOp::Mul, lr, ri),
                binop(BinaryOp::Mul, li, rr),
            );
            Some((re, im))
        }
        (Operand::Complex(lr, li), Operand::Real(s)) => Some((
            binop(BinaryOp::Mul, lr, s.clone()),
            binop(BinaryOp::Mul, li, s),
        )),
        (Operand::Real(s), Operand::Complex(rr, ri)) => Some((
            binop(BinaryOp::Mul, s.clone(), rr),
            binop(BinaryOp::Mul, s, ri),
        )),
        (Operand::Real(_), Operand::Real(_)) => None,
    }
}

fn combine_div(l: Operand, r: Operand) -> Option<(Expr, Expr)> {
    match (l, r) {
        // Only complex / real is decomposed exactly; complex/complex and
        // real/complex use Julia's numerically-careful Smith algorithm, which the
        // naive formula would not reproduce bit-for-bit — bail (stay boxed).
        (Operand::Complex(lr, li), Operand::Real(s)) => Some((
            binop(BinaryOp::Div, lr, s.clone()),
            binop(BinaryOp::Div, li, s),
        )),
        _ => None,
    }
}

/// Whether a decomposable RHS is rooted in concrete evidence (a constructor of a
/// recognized shape / an `im`-literal / a Complex param), reachable through
/// already-grounded vars.
fn rhs_is_grounded(
    e: &Expr,
    grounded: &HashSet<String>,
    complex_params: &HashSet<String>,
    shapes_by_ctor: &HashMap<String, Shape>,
    st: &ScalarTypes,
) -> bool {
    if is_f64_im_pure(e, st) {
        return true;
    }
    match e {
        Expr::Var(v, _) => grounded.contains(v.as_str()) || complex_params.contains(v.as_str()),
        Expr::Call { function, .. } if shapes_by_ctor.contains_key(function.as_str()) => true,
        Expr::Call { function, args, .. } if function == "conj" && args.len() == 1 => {
            rhs_is_grounded(&args[0], grounded, complex_params, shapes_by_ctor, st)
        }
        Expr::UnaryOp { operand, .. } => {
            rhs_is_grounded(operand, grounded, complex_params, shapes_by_ctor, st)
        }
        Expr::BinaryOp { left, right, .. } => {
            rhs_is_grounded(left, grounded, complex_params, shapes_by_ctor, st)
                || rhs_is_grounded(right, grounded, complex_params, shapes_by_ctor, st)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Total rewrite
// ---------------------------------------------------------------------------

/// Rewrite a block. Returns the new statement list, or `None` to bail SROA for
/// the whole function (an unsound construct was reached).
fn rewrite_block(block: &Block, ctx: &Ctx, counter: &mut usize) -> Option<Vec<Stmt>> {
    let mut out = Vec::with_capacity(block.stmts.len());
    for stmt in &block.stmts {
        out.extend(rewrite_stmt(stmt, ctx, counter)?);
    }
    Some(out)
}

fn rewrite_stmt(stmt: &Stmt, ctx: &Ctx, counter: &mut usize) -> Option<Vec<Stmt>> {
    match stmt {
        Stmt::Assign {
            var: v,
            value,
            span,
        } if ctx.is_split(v) => {
            let (re, im) = decompose(value, ctx)?;
            Some(emit_cf_store(v, re, im, counter, *span))
        }
        Stmt::AddAssign {
            var: v,
            value,
            span,
        } if ctx.is_split(v) => {
            let synth = binop(BinaryOp::Add, var(v.clone()), value.clone());
            let (re, im) = decompose(&synth, ctx)?;
            Some(emit_cf_store(v, re, im, counter, *span))
        }
        Stmt::Assign {
            var: v,
            value,
            span,
        } => Some(vec![Stmt::Assign {
            var: v.clone(),
            value: rewrite_expr(value, ctx)?,
            span: *span,
        }]),
        Stmt::AddAssign {
            var: v,
            value,
            span,
        } => Some(vec![Stmt::AddAssign {
            var: v.clone(),
            value: rewrite_expr(value, ctx)?,
            span: *span,
        }]),
        Stmt::For {
            var: v,
            start,
            end,
            step,
            body,
            span,
        } => Some(vec![Stmt::For {
            var: v.clone(),
            start: rewrite_expr(start, ctx)?,
            end: rewrite_expr(end, ctx)?,
            step: step.as_ref().map(|s| rewrite_expr(s, ctx)).bail()?,
            body: rewrite_child_block(body, ctx, counter)?,
            span: *span,
        }]),
        Stmt::ForEach {
            var: v,
            iterable,
            body,
            span,
        } => Some(vec![Stmt::ForEach {
            var: v.clone(),
            iterable: rewrite_expr(iterable, ctx)?,
            body: rewrite_child_block(body, ctx, counter)?,
            span: *span,
        }]),
        Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            span,
        } => Some(vec![Stmt::ForEachTuple {
            vars: vars.clone(),
            iterable: rewrite_expr(iterable, ctx)?,
            body: rewrite_child_block(body, ctx, counter)?,
            span: *span,
        }]),
        Stmt::While {
            condition,
            body,
            span,
        } => Some(vec![Stmt::While {
            condition: rewrite_expr(condition, ctx)?,
            body: rewrite_child_block(body, ctx, counter)?,
            span: *span,
        }]),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => Some(vec![Stmt::If {
            condition: rewrite_expr(condition, ctx)?,
            then_branch: rewrite_child_block(then_branch, ctx, counter)?,
            else_branch: else_branch
                .as_ref()
                .map(|b| rewrite_child_block(b, ctx, counter))
                .bail()?,
            span: *span,
        }]),
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            span,
        } => Some(vec![Stmt::Try {
            try_block: rewrite_child_block(try_block, ctx, counter)?,
            catch_var: catch_var.clone(),
            catch_block: catch_block
                .as_ref()
                .map(|b| rewrite_child_block(b, ctx, counter))
                .bail()?,
            else_block: else_block
                .as_ref()
                .map(|b| rewrite_child_block(b, ctx, counter))
                .bail()?,
            finally_block: finally_block
                .as_ref()
                .map(|b| rewrite_child_block(b, ctx, counter))
                .bail()?,
            span: *span,
        }]),
        Stmt::Block(block) => Some(vec![Stmt::Block(rewrite_child_block(block, ctx, counter)?)]),
        Stmt::Timed { body, span } => Some(vec![Stmt::Timed {
            body: rewrite_child_block(body, ctx, counter)?,
            span: *span,
        }]),
        Stmt::TestSet { name, body, span } => Some(vec![Stmt::TestSet {
            name: name.clone(),
            body: rewrite_child_block(body, ctx, counter)?,
            span: *span,
        }]),
        Stmt::Return { value, span } => Some(vec![Stmt::Return {
            value: value.as_ref().map(|e| rewrite_expr(e, ctx)).bail()?,
            span: *span,
        }]),
        Stmt::Expr { expr, span } => Some(vec![Stmt::Expr {
            expr: rewrite_expr(expr, ctx)?,
            span: *span,
        }]),
        Stmt::Test {
            condition,
            message,
            span,
        } => Some(vec![Stmt::Test {
            condition: rewrite_expr(condition, ctx)?,
            message: message.clone(),
            span: *span,
        }]),
        Stmt::TestThrows {
            exception_type,
            expr,
            span,
        } => Some(vec![Stmt::TestThrows {
            exception_type: exception_type.clone(),
            expr: Box::new(rewrite_expr(expr, ctx)?),
            span: *span,
        }]),
        Stmt::IndexAssign {
            array,
            indices,
            value,
            span,
        } => Some(vec![Stmt::IndexAssign {
            array: array.clone(),
            indices: indices
                .iter()
                .map(|i| rewrite_expr(i, ctx))
                .collect::<Option<Vec<_>>>()?,
            value: rewrite_expr(value, ctx)?,
            span: *span,
        }]),
        Stmt::FieldAssign {
            object,
            field,
            value,
            span,
        } => Some(vec![Stmt::FieldAssign {
            object: object.clone(),
            field: field.clone(),
            value: rewrite_expr(value, ctx)?,
            span: *span,
        }]),
        Stmt::DictAssign {
            dict,
            key,
            value,
            span,
        } => Some(vec![Stmt::DictAssign {
            dict: dict.clone(),
            key: rewrite_expr(key, ctx)?,
            value: rewrite_expr(value, ctx)?,
            span: *span,
        }]),
        Stmt::DestructuringAssign {
            targets,
            value,
            span,
        } => Some(vec![Stmt::DestructuringAssign {
            targets: targets.clone(),
            value: rewrite_expr(value, ctx)?,
            span: *span,
        }]),
        // FunctionDef bodies never reference a split var (captures are excluded),
        // so they pass through unchanged. Everything else is metadata.
        Stmt::FunctionDef { .. }
        | Stmt::EvalFunctionDef { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::RuntimeNominalDef { .. }
        | Stmt::Global { .. } => Some(vec![stmt.clone()]),
    }
}

/// Flip an optional-field-of-a-bail-result: an outer `Option` = "the field is
/// present", inner `Option` = "the rewrite bailed". Yields `Some(Some(v))` /
/// `Some(None)` for a present/absent field, and `None` (propagated by `?`) when
/// a present field's rewrite bailed.
fn transpose_opt<T>(x: Option<Option<T>>) -> Option<Option<T>> {
    match x {
        None => Some(None),
        Some(None) => None,
        Some(Some(v)) => Some(Some(v)),
    }
}

trait OptOptExt<T> {
    /// See [`transpose_opt`]; enables `field.as_ref().map(rewrite).bail()?`.
    fn bail(self) -> Option<Option<T>>;
}

impl<T> OptOptExt<T> for Option<Option<T>> {
    fn bail(self) -> Option<Option<T>> {
        transpose_opt(self)
    }
}

fn rewrite_child_block(block: &Block, ctx: &Ctx, counter: &mut usize) -> Option<Block> {
    Some(Block {
        stmts: rewrite_block(block, ctx, counter)?,
        span: block.span,
    })
}

/// Emit the staged store of a decomposed value into `v`'s part locals. When the
/// parts reference `v`'s own part locals (a self-referential update like
/// `z = z*z + c`), stage through fresh temporaries first so the old values are
/// fully read before either part is overwritten.
fn emit_cf_store(v: &str, re: Expr, im: Expr, counter: &mut usize, span: Span) -> Vec<Stmt> {
    let re_slot = re_name(v);
    let im_slot = im_name(v);
    let self_ref = expr_mentions(&re, &re_slot)
        || expr_mentions(&re, &im_slot)
        || expr_mentions(&im, &re_slot)
        || expr_mentions(&im, &im_slot);
    if !self_ref {
        return vec![
            Stmt::Assign {
                var: re_slot,
                value: re,
                span,
            },
            Stmt::Assign {
                var: im_slot,
                value: im,
                span,
            },
        ];
    }
    let n = *counter;
    *counter += 1;
    let tre = format!("{CX_PREFIX}t{n}_re");
    let tim = format!("{CX_PREFIX}t{n}_im");
    vec![
        Stmt::Assign {
            var: tre.clone(),
            value: re,
            span,
        },
        Stmt::Assign {
            var: tim.clone(),
            value: im,
            span,
        },
        Stmt::Assign {
            var: re_slot,
            value: var(tre),
            span,
        },
        Stmt::Assign {
            var: im_slot,
            value: var(tim),
            span,
        },
    ]
}

fn expr_mentions(e: &Expr, name: &str) -> bool {
    let mut names = HashSet::new();
    collect_names_expr(e, &mut names);
    names.contains(name)
}

/// Rewrite an expression appearing in a **real** (non-complex) context. Reuses
/// the total rewriter, which turns `real(z)`/`imag(z)`/`abs2(z)` and `z.re`/
/// `z.im`/`p.x`/`p.y` into scalar part references and never materializes (a bare
/// split var is never in a real position after `is_definitely_real`).
fn rewrite_real(e: &Expr, ctx: &Ctx) -> Option<Expr> {
    rewrite_expr(e, ctx)
}

/// The recognized 2-field shape of a decomposable object expression, if any
/// (used to map a field name to a part index).
fn decomposed_shape(e: &Expr, ctx: &Ctx) -> Option<Shape> {
    match e {
        Expr::Var(v, _) => ctx.var_shape(v),
        Expr::Call { function, .. } if ctx.shapes_by_ctor.contains_key(function.as_str()) => {
            ctx.shapes_by_ctor.get(function.as_str()).cloned()
        }
        _ if is_complex_expr(e, ctx) => Some(complex_shape()),
        _ => None,
    }
}

/// Whether `e` is a side-effect-free scalar/complex arithmetic tree, so it is
/// safe for `decompose` to (a) evaluate its leaves out of source order and
/// (b) duplicate decomposed sub-parts (the combine rules clone parts). Vars,
/// literals, field reads, arithmetic, and whitelisted pure calls (`Float64`,
/// `real`/`imag`/`abs2`/`conj`/`complex`, recognized constructors) only.
fn is_pure_arith_tree(e: &Expr, ctx: &Ctx) -> bool {
    match e {
        Expr::Var(..) | Expr::Literal(..) => true,
        Expr::UnaryOp { operand, .. } => is_pure_arith_tree(operand, ctx),
        Expr::BinaryOp { left, right, .. } => {
            is_pure_arith_tree(left, ctx) && is_pure_arith_tree(right, ctx)
        }
        Expr::FieldAccess { object, .. } => is_pure_arith_tree(object, ctx),
        Expr::Call { function, args, .. } => {
            (matches!(
                function.as_str(),
                "Float64" | "real" | "imag" | "abs2" | "conj" | "complex"
            ) || ctx.shapes_by_ctor.contains_key(function.as_str()))
                && args.iter().all(|a| is_pure_arith_tree(a, ctx))
        }
        _ => false,
    }
}

/// Total value-position rewrite. Optimized forms (`real`/`imag`/`abs2`, field
/// reads on a split object) become scalar part references; any other occurrence
/// of a split var is materialized to `T(re, im)`. Returns `None` on any construct
/// whose scoping this pass does not model soundly.
fn rewrite_expr(e: &Expr, ctx: &Ctx) -> Option<Expr> {
    // Value-position materialization (Issue #9654): a provably-Complex{Float64}
    // *arithmetic* expression not rooted at a bare split var — a call argument
    // like `cr + ci*im`, `-z` / `z*w` on decomposable operands — decomposes to
    // its real parts and rebuilds as one direct construction, replacing the
    // per-evaluation dynamic dispatch into the pure-Julia Complex method. Only
    // pure trees qualify (decompose may reorder/duplicate sub-parts), and a
    // non-decomposable expression falls through to the structural recursion
    // unchanged (e.g. `2im` stays a boxed `Complex{Int64}`).
    if matches!(e, Expr::BinaryOp { .. } | Expr::UnaryOp { .. })
        && is_complex_expr(e, ctx)
        && is_pure_arith_tree(e, ctx)
    {
        if let Some((re, im)) = decompose(e, ctx) {
            return Some(materialize("Complex{Float64}", re, im));
        }
    }
    match e {
        // --- optimized reductions (Complex-only) ---
        Expr::Call { function, args, .. }
            if function == "real"
                && args.len() == 1
                && is_complex_expr(&args[0], ctx)
                && decompose(&args[0], ctx).is_some() =>
        {
            let (re, _im) = decompose(&args[0], ctx)?;
            Some(re)
        }
        Expr::Call { function, args, .. }
            if function == "imag"
                && args.len() == 1
                && is_complex_expr(&args[0], ctx)
                && decompose(&args[0], ctx).is_some() =>
        {
            let (_re, im) = decompose(&args[0], ctx)?;
            Some(im)
        }
        Expr::Call { function, args, .. }
            if function == "abs2"
                && args.len() == 1
                && is_complex_expr(&args[0], ctx)
                && decompose(&args[0], ctx).is_some() =>
        {
            let (re, im) = decompose(&args[0], ctx)?;
            if is_simple(&re) && is_simple(&im) {
                Some(binop(
                    BinaryOp::Add,
                    binop(BinaryOp::Mul, re.clone(), re),
                    binop(BinaryOp::Mul, im.clone(), im),
                ))
            } else {
                // Non-trivial parts: fall back to materialized abs2 (correct,
                // just boxed) rather than duplicate the sub-expression.
                Some(Expr::Call {
                    function: "abs2".to_string().into(),
                    args: vec![materialize("Complex{Float64}", re, im)],
                    kwargs: Vec::new(),
                    splat_mask: vec![false],
                    kwargs_splat_mask: Vec::new(),
                    span: zero_span(),
                })
            }
        }
        // Field read on a split object -> scalar part (Complex re/im, user x/y, …).
        Expr::FieldAccess {
            object,
            field,
            span,
        } => {
            if let Some(shape) = decomposed_shape(object, ctx) {
                if let Some(idx) = shape.fields.iter().position(|f| f == field) {
                    if let Some((p0, p1)) = decompose(object, ctx) {
                        return Some(if idx == 0 { p0 } else { p1 });
                    }
                }
            }
            Some(Expr::FieldAccess {
                object: Box::new(rewrite_expr(object, ctx)?),
                field: *field,
                span: *span,
            })
        }

        // --- materialization of a bare split var (escape boundary) ---
        Expr::Var(v, _) if ctx.is_split(v) => {
            let shape = ctx.cf[v.as_str()].clone();
            Some(materialize(&shape.ctor, var(re_name(v)), var(im_name(v))))
        }

        // --- leaves (bare complex-param var stays boxed; it is not split) ---
        Expr::Var(..)
        | Expr::Literal(..)
        | Expr::FunctionRef { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => Some(e.clone()),

        // --- generic structural recursion ---
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Some(Expr::BinaryOp {
            op: *op,
            left: Box::new(rewrite_expr(left, ctx)?),
            right: Box::new(rewrite_expr(right, ctx)?),
            span: *span,
        }),
        Expr::UnaryOp { op, operand, span } => Some(Expr::UnaryOp {
            op: *op,
            operand: Box::new(rewrite_expr(operand, ctx)?),
            span: *span,
        }),
        Expr::Convert {
            target,
            operand,
            span,
        } => Some(Expr::Convert {
            target: *target,
            operand: Box::new(rewrite_expr(operand, ctx)?),
            span: *span,
        }),
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Some(Expr::Call {
            function: *function,
            args: rewrite_args(args, ctx)?,
            kwargs: rewrite_kwargs(kwargs, ctx)?,
            splat_mask: splat_mask.clone(),
            kwargs_splat_mask: kwargs_splat_mask.clone(),
            span: *span,
        }),
        Expr::ModuleCall {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Some(Expr::ModuleCall {
            module: *module,
            function: *function,
            args: rewrite_args(args, ctx)?,
            kwargs: rewrite_kwargs(kwargs, ctx)?,
            splat_mask: splat_mask.clone(),
            kwargs_splat_mask: kwargs_splat_mask.clone(),
            span: *span,
        }),
        Expr::Builtin { name, args, span } => Some(Expr::Builtin {
            name: *name,
            args: rewrite_args(args, ctx)?,
            span: *span,
        }),
        Expr::New {
            type_args,
            args,
            is_splat,
            span,
        } => Some(Expr::New {
            type_args: type_args.clone(),
            args: rewrite_args(args, ctx)?,
            is_splat: *is_splat,
            span: *span,
        }),
        Expr::ArrayLiteral {
            elements,
            shape,
            span,
        } => Some(Expr::ArrayLiteral {
            elements: rewrite_args(elements, ctx)?,
            shape: shape.clone(),
            span: *span,
        }),
        Expr::TupleLiteral { elements, span } => Some(Expr::TupleLiteral {
            elements: rewrite_args(elements, ctx)?,
            span: *span,
        }),
        Expr::Index {
            array,
            indices,
            span,
        } => Some(Expr::Index {
            array: Box::new(rewrite_expr(array, ctx)?),
            indices: rewrite_args(indices, ctx)?,
            span: *span,
        }),
        Expr::Range {
            start,
            step,
            stop,
            span,
        } => Some(Expr::Range {
            start: Box::new(rewrite_expr(start, ctx)?),
            step: step
                .as_ref()
                .map(|s| rewrite_expr(s, ctx).map(Box::new))
                .bail()?,
            stop: Box::new(rewrite_expr(stop, ctx)?),
            span: *span,
        }),
        Expr::NamedTupleLiteral { fields, span } => Some(Expr::NamedTupleLiteral {
            fields: fields
                .iter()
                .map(|(n, v)| Some((*n, rewrite_expr(v, ctx)?)))
                .collect::<Option<Vec<_>>>()?,
            span: *span,
        }),
        Expr::Pair { key, value, span } => Some(Expr::Pair {
            key: Box::new(rewrite_expr(key, ctx)?),
            value: Box::new(rewrite_expr(value, ctx)?),
            span: *span,
        }),
        Expr::DictLiteral { pairs, span } => Some(Expr::DictLiteral {
            pairs: pairs
                .iter()
                .map(|(k, v)| Some((rewrite_expr(k, ctx)?, rewrite_expr(v, ctx)?)))
                .collect::<Option<Vec<_>>>()?,
            span: *span,
        }),
        Expr::StringConcat { parts, span } => Some(Expr::StringConcat {
            parts: rewrite_args(parts, ctx)?,
            span: *span,
        }),
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            span,
        } => Some(Expr::Ternary {
            condition: Box::new(rewrite_expr(condition, ctx)?),
            then_expr: Box::new(rewrite_expr(then_expr, ctx)?),
            else_expr: Box::new(rewrite_expr(else_expr, ctx)?),
            span: *span,
        }),
        // Comprehensions/generators: a binding shadowing a split name is not
        // modeled (rare) — bail. Otherwise recurse.
        Expr::Comprehension {
            body,
            var: bv,
            iter,
            filter,
            span,
        } => {
            if ctx.is_split(bv) {
                return None;
            }
            Some(Expr::Comprehension {
                body: Box::new(rewrite_expr(body, ctx)?),
                var: *bv,
                iter: Box::new(rewrite_expr(iter, ctx)?),
                filter: filter
                    .as_ref()
                    .map(|f| rewrite_expr(f, ctx).map(Box::new))
                    .bail()?,
                span: *span,
            })
        }
        Expr::Generator {
            body,
            var: bv,
            iter,
            filter,
            span,
        } => {
            if ctx.is_split(bv) {
                return None;
            }
            Some(Expr::Generator {
                body: Box::new(rewrite_expr(body, ctx)?),
                var: *bv,
                iter: Box::new(rewrite_expr(iter, ctx)?),
                filter: filter
                    .as_ref()
                    .map(|f| rewrite_expr(f, ctx).map(Box::new))
                    .bail()?,
                span: *span,
            })
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            flatten,
            span,
        } => {
            if iterations.iter().any(|(bv, _)| ctx.is_split(bv)) {
                return None;
            }
            Some(Expr::MultiComprehension {
                body: Box::new(rewrite_expr(body, ctx)?),
                iterations: iterations
                    .iter()
                    .map(|(bv, it)| Some((*bv, rewrite_expr(it, ctx)?)))
                    .collect::<Option<Vec<_>>>()?,
                filter: filter
                    .as_ref()
                    .map(|f| rewrite_expr(f, ctx).map(Box::new))
                    .bail()?,
                flatten: *flatten,
                span: *span,
            })
        }
        Expr::DynamicTypeConstruct {
            base,
            base_expr,
            type_args,
            splat_mask,
            span,
        } => Some(Expr::DynamicTypeConstruct {
            base: *base,
            base_expr: base_expr
                .as_ref()
                .map(|b| rewrite_expr(b, ctx).map(Box::new))
                .bail()?,
            type_args: rewrite_args(type_args, ctx)?,
            splat_mask: splat_mask.clone(),
            span: *span,
        }),
        Expr::ReturnExpr { value, span } => Some(Expr::ReturnExpr {
            value: value
                .as_ref()
                .map(|v| rewrite_expr(v, ctx).map(Box::new))
                .bail()?,
            span: *span,
        }),
        // Constructs whose scoping (name rebinding mid-expression, quoting) this
        // pass does not model: if a split var occurs inside, bail; else pass through.
        Expr::LetBlock { .. } | Expr::AssignExpr { .. } | Expr::QuoteLiteral { .. } => {
            let mut names = HashSet::new();
            collect_names_expr(e, &mut names);
            if names.iter().any(|n| ctx.is_split(n)) {
                None
            } else {
                Some(e.clone())
            }
        }
    }
}

fn rewrite_args(args: &[Expr], ctx: &Ctx) -> Option<Vec<Expr>> {
    args.iter().map(|a| rewrite_expr(a, ctx)).collect()
}

fn rewrite_kwargs(
    kwargs: &[(crate::ir::core::InternedStr, Expr)],
    ctx: &Ctx,
) -> Option<Vec<(crate::ir::core::InternedStr, Expr)>> {
    kwargs
        .iter()
        .map(|(n, v)| Some((*n, rewrite_expr(v, ctx)?)))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    /// Parse + lower a snippet and return its whole lowered program.
    fn lower_program(src: &str) -> crate::ir::core::Program {
        let mut parser = Parser::new().unwrap();
        let outcome = parser.parse(src).unwrap();
        let mut lowering = Lowering::new(src);
        lowering.lower(outcome).unwrap()
    }

    /// Parse + lower a snippet, SROA its last (user) function, return that function.
    fn sroa(src: &str) -> Function {
        let program = lower_program(src);
        let shapes = build_shapes(&program.structs);
        let last = program
            .functions
            .into_iter()
            .last()
            .expect("expected at least one function");
        let mut f = (*last).clone();
        sroa_function(&mut f, &shapes);
        f
    }

    fn dbg_body(f: &Function) -> String {
        format!("{:#?}", f.body)
    }

    /// A Complex-only ctx for the unit decompose/combine tests.
    fn complex_ctx(vars: &[&str]) -> Ctx {
        let mut cf = HashMap::new();
        for v in vars {
            cf.insert((*v).to_string(), complex_shape());
        }
        let mut shapes_by_ctor = HashMap::new();
        shapes_by_ctor.insert("Complex{Float64}".to_string(), complex_shape());
        shapes_by_ctor.insert("ComplexF64".to_string(), complex_shape());
        Ctx {
            cf,
            shapes_by_ctor,
            complex_params: HashSet::new(),
            scalars: ScalarTypes::default(),
        }
    }

    #[test]
    fn im_literal_call_argument_materializes_issue_9654() {
        // Issue #9654: `cr + ci*im` in a call-argument position, with `cr`/`ci`
        // provably-Float64 locals computed from typed-int loop arithmetic, must
        // rewrite to a direct `Complex{Float64}(…)` construction (no dynamic
        // `*`/`+` dispatch per evaluation). This exercises the scalar
        // provability lattice (loop var over int bounds, f64 locals from mixed
        // f64⊗int arithmetic) and the value-position materialization.
        let f = sroa(
            "function g(width::Int64, m::Int64)::Int64\n\
             total = 0\n\
             for x in 1:width\n\
             cr = -2.0 + 3.0 * (x - 1) / (width - 1)\n\
             ci = 0.5 * (x - 1) / (width - 1)\n\
             total += h(cr + ci * im, m)\n\
             end\n\
             total\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            d.contains("Complex{Float64}"),
            "im-literal argument not materialized to a direct construction: {d}"
        );
        assert!(
            !d.contains("\"im\""),
            "dynamic `im` arithmetic survived the rewrite: {d}"
        );
    }

    #[test]
    fn unprovable_im_coefficient_argument_stays_dynamic_issue_9654() {
        // A coefficient whose type cannot be proven (untyped param) must NOT
        // decompose — `2q*im` could be Complex{Int} for an Int `q`.
        let f = sroa(
            "function g(q, m::Int64)::Int64\n\
             h(1.0 + q * im, m)\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            d.contains("\"im\""),
            "unprovable coefficient was unsoundly decomposed: {d}"
        );
    }

    #[test]
    fn impure_complex_operand_keeps_call_but_materializes_pure_leaf_issue_9654() {
        // `q(x) + 0.5im`: the outer add has an opaque call operand, so it stays
        // a dynamic dispatch; the pure `0.5im` leaf still materializes to a
        // direct `Complex{Float64}(0.0, 0.5)` (bit-identical to upstream's
        // `0.5*im`), killing the inner `*` dispatch only.
        let f = sroa(
            "function g(x::Float64, m::Int64)\n\
             h(q(x) + 0.5 * im, m)\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(d.contains("\"q\""), "opaque call operand vanished: {d}");
        assert!(
            d.contains("Complex{Float64}"),
            "pure im-literal leaf not materialized: {d}"
        );
        assert!(!d.contains("\"im\""), "inner `* im` survived: {d}");
    }

    #[test]
    fn unboxes_zzc_accumulation_loop() {
        let f = sroa(
            "function g(n::Int64)::Float64\n\
             z = Complex{Float64}(0.1, 0.2)\n\
             c = Complex{Float64}(0.1, 0.2)\n\
             i = 0\n\
             while i < n\n\
             z = z * z + c\n\
             i = i + 1\n\
             end\n\
             real(z) + imag(z)\n\
             end",
        );
        let d = dbg_body(&f);
        // The proven-ComplexF64 locals were split into f64 re/im parts …
        assert!(d.contains("__sjulia_cx_re_z"), "z not split: {d}");
        assert!(d.contains("__sjulia_cx_im_z"), "z not split");
        assert!(d.contains("__sjulia_cx_re_c"), "c not split");
        // … and no boxed Complex{Float64} construction survives (nothing escapes,
        // so there is no materialization either).
        assert!(
            !d.contains("Complex{Float64}"),
            "unexpected boxed Complex construction remained: {d}"
        );
    }

    #[test]
    fn closure_capture_forces_bail_to_boxed() {
        // A closure capturing `z` must keep it boxed (the capture would otherwise
        // reference a variable this pass removed) — z is NOT split.
        let f = sroa(
            "function g(n::Int64)::Float64\n\
             z = Complex{Float64}(0.2, 0.3)\n\
             c = Complex{Float64}(0.1, 0.1)\n\
             h = () -> real(z)\n\
             i = 0\n\
             while i < n\n\
             z = z * z + c\n\
             i = i + 1\n\
             end\n\
             real(z) + imag(z)\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            !d.contains("__sjulia_cx_re_z"),
            "captured z was unsoundly split: {d}"
        );
    }

    #[test]
    fn escape_return_materializes() {
        // Returning `z` mid-body materializes it back to Complex{Float64}(re, im).
        let f = sroa(
            "function g(n::Int64)\n\
             z = Complex{Float64}(0.4, 0.4)\n\
             c = Complex{Float64}(0.1, 0.1)\n\
             i = 0\n\
             while i < n\n\
             z = z * z + c\n\
             i = i + 1\n\
             end\n\
             z\n\
             end",
        );
        let d = dbg_body(&f);
        // z is split for the loop …
        assert!(d.contains("__sjulia_cx_re_z"), "z not split: {d}");
        // … and rematerialized at the escaping tail position.
        assert!(
            d.contains("Complex{Float64}"),
            "escape not materialized: {d}"
        );
    }

    #[test]
    fn non_complex_locals_untouched() {
        let f = sroa(
            "function g(n::Int64)::Int64\n\
             s = 0\n\
             i = 0\n\
             while i < n\n\
             s = s + i\n\
             i = i + 1\n\
             end\n\
             s\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            !d.contains("__sjulia_cx_"),
            "non-complex code rewritten: {d}"
        );
    }

    #[test]
    fn integer_complex_literal_is_not_sroad() {
        // `z = 1 + 2im` is Complex{Int64}, not Complex{Float64} — the pass must
        // NOT unbox it into f64 slots (that would change the element type).
        let f = sroa(
            "function g()\n\
             z = 1 + 2im\n\
             z = z + z\n\
             z\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            !d.contains("__sjulia_cx_"),
            "integer-complex literal was unsoundly unboxed to f64: {d}"
        );
    }

    #[test]
    fn float_imaginary_literal_init_is_sroad_issue_9198_s3() {
        // `z = 0.0 + 0.0im` has provably-Float64 coefficients ⇒ Complex{Float64};
        // it now qualifies (S3) so the loop unboxes with no boxed construction.
        let f = sroa(
            "function g(n::Int64)::Float64\n\
             z = 0.0 + 0.0im\n\
             c = Complex{Float64}(0.1, 0.2)\n\
             i = 0\n\
             while i < n\n\
             z = z * z + c\n\
             i = i + 1\n\
             end\n\
             real(z) + imag(z)\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            d.contains("__sjulia_cx_re_z"),
            "z (im-literal init) not split: {d}"
        );
        assert!(
            !d.contains("Complex{Float64}"),
            "no boxed Complex should remain: {d}"
        );
    }

    #[test]
    fn integer_imaginary_coefficient_literal_bails_issue_9198_s3() {
        // `z = 2im` is Complex{Int64} — the provably-f64 gate must reject the
        // integer coefficient, leaving z boxed.
        let f = sroa(
            "function g(n::Int64)\n\
             z = 2im\n\
             i = 0\n\
             while i < n\n\
             z = z + z\n\
             i = i + 1\n\
             end\n\
             z\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            !d.contains("__sjulia_cx_"),
            "integer-imaginary literal 2im was unsoundly unboxed: {d}"
        );
    }

    #[test]
    fn complex_f64_param_is_decomposed_operand_issue_9198_s3() {
        // A boxed ::ComplexF64 param used in `z = z*z + c` decomposes (its re/im
        // are hoisted to f64 locals at entry), so z fully unboxes.
        let f = sroa(
            "function mandel(c::ComplexF64, maxiter::Int64)::Int64\n\
             z = 0.0 + 0.0im\n\
             k = 0\n\
             while k < maxiter\n\
             z = z * z + c\n\
             k = k + 1\n\
             end\n\
             k\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(d.contains("__sjulia_cx_re_z"), "z not split: {d}");
        // The param's re/im are hoisted to part locals.
        assert!(
            d.contains("__sjulia_cx_re_c"),
            "param c re not hoisted: {d}"
        );
        assert!(
            d.contains("__sjulia_cx_im_c"),
            "param c im not hoisted: {d}"
        );
    }

    #[test]
    fn user_two_field_f64_struct_is_sroad_issue_9198_s3() {
        // A user immutable struct with two Float64 fields unboxes for the
        // construct + field-read shape (no built-in arithmetic needed).
        let f = sroa(
            "struct V2\n\
             x::Float64\n\
             y::Float64\n\
             end\n\
             function march(n::Int64)::Float64\n\
             p = V2(0.0, 0.0)\n\
             i = 0\n\
             while i < n\n\
             p = V2(p.x + 1.0, p.y + 2.0)\n\
             i = i + 1\n\
             end\n\
             p.x + p.y\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(d.contains("__sjulia_cx_re_p"), "p not split: {d}");
        assert!(
            d.contains("__sjulia_cx_im_p"),
            "p not split (2nd field): {d}"
        );
        // No boxed V2 construction survives (nothing escapes).
        assert!(!d.contains("\"V2\""), "unexpected boxed V2 remained: {d}");
    }

    #[test]
    fn user_struct_operator_method_call_stays_boxed_issue_9198_s3() {
        // A user `+` method on the struct is NOT inlined here, so `p = p + q`
        // does not decompose — the local stays boxed (honest limitation).
        let f = sroa(
            "struct V2\n\
             x::Float64\n\
             y::Float64\n\
             end\n\
             import Base: +\n\
             +(a::V2, b::V2) = V2(a.x + b.x, a.y + b.y)\n\
             function march(n::Int64)::Float64\n\
             p = V2(0.0, 0.0)\n\
             q = V2(1.0, 2.0)\n\
             i = 0\n\
             while i < n\n\
             p = p + q\n\
             i = i + 1\n\
             end\n\
             p.x + p.y\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            !d.contains("__sjulia_cx_re_p"),
            "p (assigned via a user + method) should stay boxed: {d}"
        );
    }

    #[test]
    fn mutable_two_field_f64_struct_is_not_sroad() {
        // A mutable struct has identity/aliasing semantics (StructRef) — must NOT
        // be split.
        let f = sroa(
            "mutable struct M2\n\
             x::Float64\n\
             y::Float64\n\
             end\n\
             function march(n::Int64)::Float64\n\
             p = M2(0.0, 0.0)\n\
             i = 0\n\
             while i < n\n\
             p = M2(p.x + 1.0, p.y + 2.0)\n\
             i = i + 1\n\
             end\n\
             p.x + p.y\n\
             end",
        );
        let d = dbg_body(&f);
        assert!(
            !d.contains("__sjulia_cx_re_p"),
            "mutable struct was unsoundly split: {d}"
        );
    }

    #[test]
    fn decompose_matches_upstream_mul_formula() {
        // (a+bi)(c+di) = (ac-bd) + (ad+bc)i — same op order as Base `*`.
        let ctx = complex_ctx(&["z", "w"]);
        let expr = binop(BinaryOp::Mul, var("z".to_string()), var("w".to_string()));
        let (re, im) = decompose(&expr, &ctx).expect("z*w must decompose");
        // re = z_re*w_re - z_im*w_im
        assert!(matches!(
            re,
            Expr::BinaryOp {
                op: BinaryOp::Sub,
                ..
            }
        ));
        // im = z_re*w_im + z_im*w_re
        assert!(matches!(
            im,
            Expr::BinaryOp {
                op: BinaryOp::Add,
                ..
            }
        ));
    }

    #[test]
    fn division_by_complex_bails_but_by_real_decomposes() {
        let ctx = complex_ctx(&["z", "w"]);
        // z / w (complex / complex) → not decomposed (Julia's careful algorithm).
        let cc = binop(BinaryOp::Div, var("z".to_string()), var("w".to_string()));
        assert!(decompose(&cc, &ctx).is_none());
        // z / 2.0 (complex / real) → decomposed.
        let cr = binop(BinaryOp::Div, var("z".to_string()), f64_lit(2.0));
        assert!(decompose(&cr, &ctx).is_some());
    }
}
