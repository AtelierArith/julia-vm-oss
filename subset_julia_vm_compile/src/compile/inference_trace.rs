//! Developer-facing inference trace entrypoint (Issue #3512).
//!
//! Provides a way to run the abstract-interpretation inference engine on a
//! single function while capturing a step-by-step trace of inference state:
//! incoming argument types, per-statement type updates, environment changes
//! at branch points, recursive-cycle / limited-accuracy events, and the
//! final inferred return type.
//!
//! This module is **purely observational** — enabling the trace MUST NOT
//! change the result of inference. The collector uses thread-local storage
//! and opt-in `enable()`/`disable()` so that normal compilation pays no cost.
//!
//! # Comparable Julia entrypoints
//!
//! See `julia/Compiler/src/typeinfer.jl::typeinf_code` and
//! `julia/doc/src/devdocs/inference.md`. Like `typeinf_code`, our trace
//! takes a target function + argument types and dumps the inference state
//! as it runs. Unlike Julia, we don't yet expose a `MethodInstance` or
//! `CodeInfo` form — the trace is over the existing Core IR statements.
//!
//! # Example
//!
//! ```no_run
//! use subset_julia_vm::compile::inference_trace::{infer_with_trace, TraceOptions};
//! use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
//! # use subset_julia_vm::ir::core::{Block, Expr, Function, Literal, Stmt, TypedParam};
//! # use subset_julia_vm::span::Span;
//! # let dummy = Span::new(0, 0, 0, 0, 0, 0);
//! # let func = Function {
//! #     name: "f".to_string(),
//! #     params: vec![TypedParam { name: "x".into(), type_annotation: None,
//! #         is_varargs: false, vararg_count: None, span: dummy }],
//! #     kwparams: vec![], type_params: vec![], return_type: None,
//! #     body: Block { stmts: vec![Stmt::Return { value: Some(Expr::Var("x".into(), dummy)),
//! #                                              span: dummy }], span: dummy },
//! #     is_base_extension: false,
//! #     is_runtime_eval: false, span: dummy,
//! # };
//! let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)))];
//! let report = infer_with_trace(&func, &argtypes, &[], TraceOptions::default());
//! assert_eq!(report.return_type, LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
//! ```

#[cfg(test)]
use crate::inference_core::{CorePrimitive, CoreType};
use std::cell::RefCell;
use std::collections::HashMap;

use crate::compile::abstract_interp::{InferenceEngine, StructTypeInfo, TypeEnv};
use crate::compile::diagnostics::{DiagnosticsCollector, TypeInferenceDiagnostic};
use crate::compile::lattice::types::LatticeType;
use crate::ir::core::{Function, Stmt};
use crate::span::Span;

/// Options controlling how a trace is collected and rendered.
#[derive(Clone, Debug, Default)]
pub struct TraceOptions {
    /// If true, prefer JSON-friendly rendering when calling
    /// [`TraceReport::to_json_string`]; otherwise human-readable.
    pub json: bool,
}

/// One observed event during inference.
///
/// Events are produced in the order they occur during the
/// abstract-interpretation walk. They are intentionally coarse-grained
/// (function entry / per-statement / branch / cycle / return) so that the
/// trace stays readable for a developer skimming a small function.
#[derive(Clone, Debug)]
pub enum TraceEvent {
    /// The engine started inferring a function with the given argument types.
    FunctionEntry {
        function: String,
        arg_types: Vec<(String, LatticeType)>,
    },
    /// A top-level statement in the body finished being processed.
    /// `env_after` is the engine's type environment after the statement.
    Statement {
        index: usize,
        kind: &'static str,
        span: Span,
        env_after: Vec<(String, LatticeType)>,
    },
    /// A branch point — captures the inferred environments going into the
    /// then- and else- branches (post-narrowing) so that callers can see
    /// where conditional refinement diverged.
    Branch {
        kind: BranchKind,
        then_env: Vec<(String, LatticeType)>,
        else_env: Vec<(String, LatticeType)>,
    },
    /// A recursive cycle was observed for the named function(s).
    RecursiveCycle { functions: Vec<String> },
    /// Inference widened a result conservatively (e.g. union too large,
    /// fixed-point divergence). Mirrors the `DiagnosticReason` text so that
    /// the trace does not duplicate the diagnostics enum.
    LimitedAccuracy {
        reason: String,
        context: Option<String>,
    },
    /// The final inferred return type for this function call.
    Return { return_type: LatticeType },
}

/// Coarse classification for [`TraceEvent::Branch`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchKind {
    /// `if`/`elseif`/`else`
    If,
    /// `try`/`catch`/`finally`
    Try,
}

/// Aggregate result returned by [`infer_with_trace`].
#[derive(Clone, Debug)]
pub struct TraceReport {
    /// Function name that was inferred.
    pub function: String,
    /// Argument types as bound to the function's parameters.
    pub arg_types: Vec<(String, LatticeType)>,
    /// Final inferred return type — equal to running the engine without the
    /// trace enabled.
    pub return_type: LatticeType,
    /// Ordered events captured during inference.
    pub events: Vec<TraceEvent>,
    /// Diagnostics emitted during inference (snapshot of
    /// `DiagnosticsCollector::take`). Always populated, regardless of
    /// whether diagnostics were globally enabled before the call —
    /// `infer_with_trace` enables them for the duration of the run.
    pub diagnostics: Vec<TypeInferenceDiagnostic>,
}

impl TraceReport {
    /// Render the report as plain text for human consumption.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("=== inference trace: {} ===\n", self.function));
        out.push_str("arg types:\n");
        if self.arg_types.is_empty() {
            out.push_str("  (no arguments)\n");
        } else {
            for (name, ty) in &self.arg_types {
                out.push_str(&format!("  {}: {:?}\n", name, ty));
            }
        }
        out.push_str("events:\n");
        for ev in &self.events {
            match ev {
                TraceEvent::FunctionEntry {
                    function,
                    arg_types,
                } => {
                    out.push_str(&format!("  - enter {}({})\n", function, fmt_env(arg_types)));
                }
                TraceEvent::Statement {
                    index,
                    kind,
                    span,
                    env_after,
                } => {
                    out.push_str(&format!(
                        "  - stmt #{} {} @line {}: env={{{}}}\n",
                        index,
                        kind,
                        span.start_line,
                        fmt_env(env_after)
                    ));
                }
                TraceEvent::Branch {
                    kind,
                    then_env,
                    else_env,
                } => {
                    out.push_str(&format!(
                        "  - branch {:?}: then={{{}}} else={{{}}}\n",
                        kind,
                        fmt_env(then_env),
                        fmt_env(else_env)
                    ));
                }
                TraceEvent::RecursiveCycle { functions } => {
                    out.push_str(&format!("  - cycle: {}\n", functions.join(" -> ")));
                }
                TraceEvent::LimitedAccuracy { reason, context } => {
                    if let Some(ctx) = context {
                        out.push_str(&format!("  - limited: {} ({})\n", reason, ctx));
                    } else {
                        out.push_str(&format!("  - limited: {}\n", reason));
                    }
                }
                TraceEvent::Return { return_type } => {
                    out.push_str(&format!("  - return {:?}\n", return_type));
                }
            }
        }
        out.push_str(&format!("final return: {:?}\n", self.return_type));
        if !self.diagnostics.is_empty() {
            out.push_str("diagnostics:\n");
            for d in &self.diagnostics {
                out.push_str(&format!("  - {}\n", d));
            }
        }
        out
    }

    /// Render the report as a JSON string. Uses the existing serde impls
    /// for [`LatticeType`].
    pub fn to_json_string(&self) -> String {
        // Build a serde_json::Value manually so we don't have to derive
        // Serialize on the IR/Span types or pollute the engine module.
        use serde_json::{json, Value};

        let arg_types: Vec<Value> = self
            .arg_types
            .iter()
            .map(
                |(n, t)| json!({"name": n, "type": serde_json::to_value(t).unwrap_or(Value::Null)}),
            )
            .collect();

        let events: Vec<Value> = self
            .events
            .iter()
            .map(|ev| match ev {
                TraceEvent::FunctionEntry {
                    function,
                    arg_types,
                } => json!({
                    "kind": "function_entry",
                    "function": function,
                    "arg_types": serialize_env(arg_types),
                }),
                TraceEvent::Statement {
                    index,
                    kind,
                    span,
                    env_after,
                } => json!({
                    "kind": "statement",
                    "index": index,
                    "stmt": kind,
                    "line": span.start_line,
                    "column": span.start_column,
                    "env": serialize_env(env_after),
                }),
                TraceEvent::Branch {
                    kind,
                    then_env,
                    else_env,
                } => json!({
                    "kind": "branch",
                    "branch": format!("{:?}", kind),
                    "then_env": serialize_env(then_env),
                    "else_env": serialize_env(else_env),
                }),
                TraceEvent::RecursiveCycle { functions } => json!({
                    "kind": "recursive_cycle",
                    "functions": functions,
                }),
                TraceEvent::LimitedAccuracy { reason, context } => json!({
                    "kind": "limited_accuracy",
                    "reason": reason,
                    "context": context,
                }),
                TraceEvent::Return { return_type } => json!({
                    "kind": "return",
                    "return_type": serde_json::to_value(return_type).unwrap_or(Value::Null),
                }),
            })
            .collect();

        let diagnostics: Vec<Value> = self
            .diagnostics
            .iter()
            .map(|d| {
                json!({
                    "message": d.to_string(),
                    "widened_to": d.widened_to,
                })
            })
            .collect();

        let root = json!({
            "function": self.function,
            "arg_types": arg_types,
            "return_type": serde_json::to_value(&self.return_type).unwrap_or(Value::Null),
            "events": events,
            "diagnostics": diagnostics,
        });
        serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string())
    }
}

fn serialize_env(env: &[(String, LatticeType)]) -> serde_json::Value {
    use serde_json::json;
    let entries: Vec<serde_json::Value> = env
        .iter()
        .map(|(n, t)| {
            json!({
                "name": n,
                "type": serde_json::to_value(t).unwrap_or(serde_json::Value::Null),
            })
        })
        .collect();
    entries.into()
}

fn fmt_env(env: &[(String, LatticeType)]) -> String {
    let parts: Vec<String> = env.iter().map(|(n, t)| format!("{}: {:?}", n, t)).collect();
    parts.join(", ")
}

// ---------------------------------------------------------------------------
// Thread-local collector
// ---------------------------------------------------------------------------

thread_local! {
    static TRACER_ENABLED: RefCell<bool> = const { RefCell::new(false) };
    static TRACE_EVENTS: RefCell<Vec<TraceEvent>> = const { RefCell::new(Vec::new()) };
}

/// Thread-local trace collector used by the abstract-interpretation engine.
///
/// The engine emits events via the helper free functions in this module
/// (`record_*`). When the collector is disabled (the default) the helpers
/// are a no-op. This keeps the regular compilation path zero-cost.
#[derive(Debug)]
pub struct InferenceTracer;

impl InferenceTracer {
    /// Enable trace collection on this thread.
    pub fn enable() {
        TRACER_ENABLED.with(|e| *e.borrow_mut() = true);
    }

    /// Disable trace collection on this thread.
    pub fn disable() {
        TRACER_ENABLED.with(|e| *e.borrow_mut() = false);
    }

    /// True if trace collection is enabled on this thread.
    pub fn is_enabled() -> bool {
        TRACER_ENABLED.with(|e| *e.borrow())
    }

    /// Take all collected events, clearing the per-thread buffer.
    pub fn take() -> Vec<TraceEvent> {
        TRACE_EVENTS.with(|buf| std::mem::take(&mut *buf.borrow_mut()))
    }

    /// Drop all collected events without returning them.
    pub fn clear() {
        TRACE_EVENTS.with(|buf| buf.borrow_mut().clear());
    }
}

/// Append an event to the per-thread trace buffer (no-op if disabled).
pub(crate) fn record_event(event: TraceEvent) {
    if !InferenceTracer::is_enabled() {
        return;
    }
    TRACE_EVENTS.with(|buf| buf.borrow_mut().push(event));
}

/// Convenience: snapshot a [`TypeEnv`] into a stable, sorted vector so that
/// the recorded events do not depend on hash-map iteration order.
pub(crate) fn snapshot_env(env: &TypeEnv) -> Vec<(String, LatticeType)> {
    let mut entries: Vec<(String, LatticeType)> = env
        .vars()
        .cloned()
        .filter_map(|name| env.get(&name).cloned().map(|ty| (name, ty)))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

// ---------------------------------------------------------------------------
// Statement-kind helpers
// ---------------------------------------------------------------------------

/// Return a stable, human-readable tag for a statement variant. Used by the
/// trace so the developer can scan the events without needing to dig into
/// the IR types.
pub(crate) fn stmt_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Block(_) => "block",
        Stmt::Assign { .. } => "assign",
        Stmt::AddAssign { .. } => "add_assign",
        Stmt::For { .. } => "for",
        Stmt::ForEach { .. } => "foreach",
        Stmt::ForEachTuple { .. } => "foreach_tuple",
        Stmt::While { .. } => "while",
        Stmt::If { .. } => "if",
        Stmt::Return { .. } => "return",
        Stmt::Expr { .. } => "expr",
        Stmt::Break { .. } => "break",
        Stmt::Continue { .. } => "continue",
        Stmt::Try { .. } => "try",
        _ => "other",
    }
}

// ---------------------------------------------------------------------------
// Public driver
// ---------------------------------------------------------------------------

/// Run inference on a single function with tracing enabled and return a
/// structured [`TraceReport`].
///
/// `arg_types` is bound positionally to the function's parameters
/// (additional parameters fall back to their declared annotation or `Top`,
/// matching the engine's normal binding rules). `extra_functions` lets the
/// caller register additional functions for interprocedural / recursive
/// resolution — typical use is to pass the current program's `functions`
/// vector when tracing a recursive function.
///
/// This function:
/// 1. Saves and restores `DiagnosticsCollector` state, so callers don't
///    have to.
/// 2. Snapshots the engine's type environment after every top-level
///    statement of the body and after each branch split.
/// 3. Guarantees that the returned `return_type` is identical to running
///    the engine with the trace disabled (purely observational).
pub fn infer_with_trace(
    func: &Function,
    arg_types: &[LatticeType],
    extra_functions: &[Function],
    _options: TraceOptions,
) -> TraceReport {
    infer_with_trace_full(func, arg_types, extra_functions, HashMap::new(), _options)
}

/// Variant of [`infer_with_trace`] that lets the caller supply a struct
/// table for inference. Most callers should prefer `infer_with_trace`.
pub fn infer_with_trace_full(
    func: &Function,
    arg_types: &[LatticeType],
    extra_functions: &[Function],
    struct_table: HashMap<String, StructTypeInfo>,
    _options: TraceOptions,
) -> TraceReport {
    // Snapshot prior collector state so we can restore it. Diagnostics are
    // re-enabled for the duration of the run so the report can include
    // any widening events that fire during this trace.
    let prior_diag_enabled = DiagnosticsCollector::is_enabled();
    let prior_diag = DiagnosticsCollector::take();

    DiagnosticsCollector::enable();
    InferenceTracer::clear();
    InferenceTracer::enable();

    let mut engine = InferenceEngine::with_tables(
        struct_table,
        extra_functions
            .iter()
            .cloned()
            .map(|f| (f.name.clone(), f))
            .collect(),
    );

    // Compute and emit the bound argument types up-front so that the
    // report has a stable record even if the engine returns from cache
    // before processing any statements.
    let bound_args = bind_for_trace(func, arg_types);
    record_event(TraceEvent::FunctionEntry {
        function: func.name.clone(),
        arg_types: bound_args.clone(),
    });

    let return_type = engine.infer_function_with_arg_types(func, arg_types);

    record_event(TraceEvent::Return {
        return_type: return_type.clone(),
    });

    let events = InferenceTracer::take();
    InferenceTracer::disable();

    let diagnostics = DiagnosticsCollector::take();

    // Restore caller's diagnostics state. We deliberately do *not* restore
    // the prior diagnostics buffer — it was already taken above and we
    // don't want to silently merge state across users.
    if !prior_diag_enabled {
        DiagnosticsCollector::disable();
    }
    if !prior_diag.is_empty() {
        // Re-emit prior diagnostics so the surrounding caller sees them.
        // Re-enabling here is required because emit() is a no-op when
        // disabled.
        let was_enabled = DiagnosticsCollector::is_enabled();
        if !was_enabled {
            DiagnosticsCollector::enable();
        }
        for d in prior_diag {
            DiagnosticsCollector::emit(d);
        }
        if !was_enabled {
            DiagnosticsCollector::disable();
        }
    }

    TraceReport {
        function: func.name.clone(),
        arg_types: bound_args,
        return_type,
        events,
        diagnostics,
    }
}

/// Bind argument lattice types positionally to parameter names. Mirrors
/// the engine's `bind_call_args_to_params` behavior closely enough to
/// produce the same names; we keep this local copy minimal because the
/// engine's helper is private.
fn bind_for_trace(func: &Function, arg_types: &[LatticeType]) -> Vec<(String, LatticeType)> {
    func.params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let ty = arg_types.get(i).cloned().unwrap_or(LatticeType::Top);
            (p.name.clone(), ty)
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;
    use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, TypedParam};
    use crate::span::Span;

    fn dummy() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn line_span(line: usize) -> Span {
        Span::new(0, 0, line, 1, line, 10)
    }

    fn simple_func() -> Function {
        // f(x::Int) = x + 1
        Function {
            name: "f".into(),
            params: vec![TypedParam {
                name: "x".into(),
                type_annotation: None,
                is_varargs: false,
                vararg_count: None,
                span: dummy(),
            }],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".into(), line_span(2))),
                        right: Box::new(Expr::Literal(Literal::Int(1), line_span(2))),
                        span: line_span(2),
                    }),
                    span: line_span(2),
                }],
                span: dummy(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy(),
            new_struct_name: None,
        }
    }

    fn branchy_func() -> Function {
        // function g(x::Int)
        //     if x > 0
        //         y = 1
        //     else
        //         y = 2.0
        //     end
        //     return y
        // end
        Function {
            name: "g".into(),
            params: vec![TypedParam {
                name: "x".into(),
                type_annotation: None,
                is_varargs: false,
                vararg_count: None,
                span: dummy(),
            }],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![
                    Stmt::If {
                        condition: Expr::BinaryOp {
                            op: BinaryOp::Gt,
                            left: Box::new(Expr::Var("x".into(), line_span(2))),
                            right: Box::new(Expr::Literal(Literal::Int(0), line_span(2))),
                            span: line_span(2),
                        },
                        then_branch: Block {
                            stmts: vec![Stmt::Assign {
                                var: "y".into(),
                                value: Expr::Literal(Literal::Int(1), line_span(3)),
                                span: line_span(3),
                            }],
                            span: line_span(3),
                        },
                        else_branch: Some(Block {
                            stmts: vec![Stmt::Assign {
                                var: "y".into(),
                                value: Expr::Literal(Literal::Float(2.0), line_span(5)),
                                span: line_span(5),
                            }],
                            span: line_span(5),
                        }),
                        span: line_span(2),
                    },
                    Stmt::Return {
                        value: Some(Expr::Var("y".into(), line_span(7))),
                        span: line_span(7),
                    },
                ],
                span: dummy(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy(),
            new_struct_name: None,
        }
    }

    fn recursive_func() -> Function {
        // function r(n::Int)
        //     if n <= 1
        //         return 1
        //     end
        //     return r(n - 1)
        // end
        Function {
            name: "r".into(),
            params: vec![TypedParam {
                name: "n".into(),
                type_annotation: None,
                is_varargs: false,
                vararg_count: None,
                span: dummy(),
            }],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![
                    Stmt::If {
                        condition: Expr::BinaryOp {
                            op: BinaryOp::Le,
                            left: Box::new(Expr::Var("n".into(), line_span(2))),
                            right: Box::new(Expr::Literal(Literal::Int(1), line_span(2))),
                            span: line_span(2),
                        },
                        then_branch: Block {
                            stmts: vec![Stmt::Return {
                                value: Some(Expr::Literal(Literal::Int(1), line_span(3))),
                                span: line_span(3),
                            }],
                            span: line_span(3),
                        },
                        else_branch: None,
                        span: line_span(2),
                    },
                    Stmt::Return {
                        value: Some(Expr::Call {
                            function: "r".into(),
                            args: vec![Expr::BinaryOp {
                                op: BinaryOp::Sub,
                                left: Box::new(Expr::Var("n".into(), line_span(5))),
                                right: Box::new(Expr::Literal(Literal::Int(1), line_span(5))),
                                span: line_span(5),
                            }],
                            kwargs: vec![],
                            splat_mask: vec![false],
                            kwargs_splat_mask: vec![],
                            span: line_span(5),
                        }),
                        span: line_span(5),
                    },
                ],
                span: dummy(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy(),
            new_struct_name: None,
        }
    }

    /// Issue #3512 acceptance test: simple function trace contains the
    /// final return type.
    #[test]
    fn trace_simple_function_returns_int64() {
        let func = simple_func();
        let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let report = infer_with_trace(&func, &argtypes, &[], TraceOptions::default());

        assert_eq!(
            report.return_type,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        // Function entry + at least one statement + return.
        assert!(report.events.len() >= 3, "events: {:?}", report.events);
        assert!(matches!(
            report.events.first(),
            Some(TraceEvent::FunctionEntry { .. })
        ));
        assert!(matches!(
            report.events.last(),
            Some(TraceEvent::Return { .. })
        ));
        // x is bound to Int64.
        assert_eq!(report.arg_types.len(), 1);
        assert_eq!(report.arg_types[0].0, "x");
        assert_eq!(
            report.arg_types[0].1,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    /// Issue #3512 acceptance test: branchy function — both branches'
    /// environments appear in the trace.
    #[test]
    fn trace_branchy_function_records_both_branches() {
        let func = branchy_func();
        let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let report = infer_with_trace(&func, &argtypes, &[], TraceOptions::default());

        let mut saw_branch_then_y_int = false;
        let mut saw_branch_else_y_float = false;
        for ev in &report.events {
            if let TraceEvent::Branch {
                then_env, else_env, ..
            } = ev
            {
                for (name, ty) in then_env {
                    if name == "y"
                        && matches!(
                            ty,
                            LatticeType::Const(_)
                                | LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                    CorePrimitive::Int64
                                )))
                        )
                    {
                        saw_branch_then_y_int = true;
                    }
                }
                for (name, ty) in else_env {
                    if name == "y"
                        && matches!(
                            ty,
                            LatticeType::Const(_)
                                | LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                                    CorePrimitive::Float64
                                )))
                        )
                    {
                        saw_branch_else_y_float = true;
                    }
                }
            }
        }
        assert!(
            saw_branch_then_y_int,
            "expected then-branch env entry y :: Int64; events={:?}",
            report.events
        );
        assert!(
            saw_branch_else_y_float,
            "expected else-branch env entry y :: Float64; events={:?}",
            report.events
        );
    }

    /// Issue #3512 acceptance test: recursive function emits a cycle event.
    #[test]
    fn trace_recursive_function_emits_cycle_event() {
        let func = recursive_func();
        let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        // Make `r` callable from inside its own body.
        let report = infer_with_trace(
            &func,
            &argtypes,
            std::slice::from_ref(&func),
            TraceOptions::default(),
        );

        let cycle = report
            .events
            .iter()
            .any(|e| matches!(e, TraceEvent::RecursiveCycle { .. }));
        assert!(
            cycle,
            "expected RecursiveCycle event; events={:?}",
            report.events
        );
    }

    /// Trace must not change inference results: the return type with
    /// tracing on must equal the return type with tracing off.
    #[test]
    fn trace_is_observational_only() {
        let func = branchy_func();
        let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];

        // With tracing.
        let with_trace =
            infer_with_trace(&func, &argtypes, &[], TraceOptions::default()).return_type;

        // Without tracing — directly use the engine.
        let mut engine = InferenceEngine::new();
        let without_trace = engine.infer_function_with_arg_types(&func, &argtypes);

        assert_eq!(with_trace, without_trace, "trace changed inference result");
    }

    /// Same observational property for the recursive case (which exercises
    /// the cycle path).
    #[test]
    fn trace_observational_for_recursion() {
        let func = recursive_func();
        let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];

        let with_trace = infer_with_trace(
            &func,
            &argtypes,
            std::slice::from_ref(&func),
            TraceOptions::default(),
        )
        .return_type;
        let mut engine = InferenceEngine::new();
        engine.add_function(func.clone());
        let without_trace = engine.infer_function_with_arg_types(&func, &argtypes);

        assert_eq!(with_trace, without_trace);
    }

    /// The text rendering should mention the function name and the final
    /// return type so it is useful for a developer skim.
    #[test]
    fn trace_text_rendering_contains_function_and_return() {
        let func = simple_func();
        let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let report = infer_with_trace(&func, &argtypes, &[], TraceOptions::default());
        let text = report.to_text();
        assert!(text.contains("inference trace: f"), "text: {}", text);
        assert!(text.contains("Int64"), "text: {}", text);
    }

    /// JSON rendering should be valid JSON with a `function` key.
    #[test]
    fn trace_json_rendering_is_valid() {
        let func = simple_func();
        let argtypes = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let report = infer_with_trace(&func, &argtypes, &[], TraceOptions::default());
        let json = report.to_json_string();
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("trace JSON should parse");
        assert_eq!(parsed["function"], "f");
        assert!(parsed["events"].is_array());
    }
}
