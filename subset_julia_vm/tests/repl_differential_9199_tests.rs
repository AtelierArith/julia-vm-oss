//! Golden REPL harness — the retained safety net after the REPL eval-model
//! migration (Issue #9199 / retirement Issue #9784).
//!
//! # What this is
//!
//! It drives each input sequence through the production persistent `REPLSession`
//! and asserts the displayed value, type tag, captured stdout, and error class
//! against recorded goldens reviewed against upstream Julia. A separate run
//! compares two independent sessions to retain the former harness's determinism
//! coverage without keeping Legacy as an executable oracle.
//!
//! # Coverage
//!
//! - The **existing** `tests/fixtures/repl_session/*.toml` corpora (#8714, and the
//!   #9156/#9157/#9172/#9173/#9182 regression sessions) are reused verbatim as
//!   input sequences — scalars/globals, method (re)definition across evals,
//!   structs, `begin`/`let` scope, `@variables`, errors, display forms.
//! - A dedicated **migration-seam** corpus
//!   (`tests/fixtures/repl_differential/migration_seams_9199.toml`) that S3+ will
//!   extend, adding closures, `using`/`import`, macro persistence, and an explicit
//!   `reset` action with golden expectations.
//! - A value-carried-container corpus
//!   (`tests/fixtures/repl_differential/value_carried_containers_9793.toml`) that
//!   pins Tuple/NamedTuple/Pair globals assigned in one eval and consumed by
//!   container operations in later evals (the #9786 bug class).
//!
//! # Observation projection
//!
//! Observations come only from the **public** REPL API (`REPLSession::eval`,
//! `REPLResult`), so this test compiles against `subset_julia_vm` like any host.
//! `type` is a Julia type name for the common concrete scalars and a stable
//! `Value`-variant tag otherwise (a coarse type-class proxy); combined with the
//! exact `display` string it is a solid behavioral projection. The `Value` enum
//! itself survives the migration, so this projection stays meaningful across S3+.
//!
//! Set `REPL_DIFF_DUMP=1` to print each migration-seam observation (side A) to
//! stderr — used to author/refresh the corpus goldens after a deliberate change.

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use subset_julia_vm::repl::{REPLResult, REPLSession};
use subset_julia_vm_bytecode::value::StructInstance;
use subset_julia_vm_bytecode::Value;

// ---------------------------------------------------------------------------
// Observation: the model-independent projection compared across the two sides.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Observation {
    success: bool,
    stdout: String,
    /// Echoed value display (`None` when suppressed with `;` or there is no value).
    display: Option<String>,
    /// Julia type of the result value (`None` when there is no value).
    type_repr: Option<String>,
    /// Full error text (`None` on success). Compared exactly across sides; goldens
    /// only ever match a substring of it (`expect_error_contains`).
    error: Option<String>,
}

impl Observation {
    /// Sentinel for a non-eval action (e.g. `reset`). Two resets always match.
    fn action_marker() -> Self {
        Observation {
            success: true,
            stdout: String::new(),
            display: None,
            type_repr: None,
            error: None,
        }
    }
}

/// Julia type name for common concrete scalars; a stable `Value`-variant tag for
/// everything else (structs, functions, arrays, tuples, ranges, …).
///
/// `heap` resolves a heap-indirected `Value::StructRef` to its faithful struct
/// name. A struct value can surface either inline (`Value::Struct`, e.g. a
/// source-reconstructed global) or as a heap reference (`Value::StructRef`, e.g.
/// a carried struct and the VM's normal array-of-structs
/// element carrier) — the same user-visible Julia type either way. Resolving both
/// to the struct name keeps the type proxy carrier-independent, so an inline vs
/// heap-referenced struct is not a spurious divergence (Issue #9199 S3).
fn julia_type_repr(v: &Value, heap: &[StructInstance]) -> String {
    if let Value::StructRef(idx) = v {
        return heap
            .get(*idx)
            .map(|si| si.struct_name.to_string())
            .unwrap_or_else(|| variant_tag(v));
    }
    let concrete = match v {
        Value::I64(_) => "Int64",
        Value::I32(_) => "Int32",
        Value::I16(_) => "Int16",
        Value::I8(_) => "Int8",
        Value::I128(_) => "Int128",
        Value::U64(_) => "UInt64",
        Value::U32(_) => "UInt32",
        Value::U16(_) => "UInt16",
        Value::U8(_) => "UInt8",
        Value::U128(_) => "UInt128",
        Value::F64(_) => "Float64",
        Value::F32(_) => "Float32",
        Value::F16(_) => "Float16",
        Value::Bool(_) => "Bool",
        Value::Char(_) => "Char",
        Value::Str(_) => "String",
        Value::Nothing => "Nothing",
        Value::Missing => "Missing",
        // Arrays and user structs both surface as `Value::Struct`; the carried
        // `struct_name` is the faithful Julia type name ("Array{Int64, 1}",
        // "Pt9199"), far better than a bare "Struct" variant tag.
        Value::Struct(si) => return si.struct_name.to_string(),
        _ => return variant_tag(v),
    };
    concrete.to_string()
}

/// The outer `Value` variant name, e.g. `Function`, `StructRef`, `Tuple`, `Array`.
fn variant_tag(v: &Value) -> String {
    let debug = format!("{v:?}");
    debug
        .split(['(', ' ', '{', '['])
        .next()
        .unwrap_or(&debug)
        .to_string()
}

/// Coarse error class = text up to the first `:` (e.g. `UndefVarError`,
/// `BoundsError`, `MethodError`, `Compile error`). Used only for reporting and
/// substring goldens; the full text is what side A vs side B compares.
fn classify_error(err: &str) -> String {
    err.split(':').next().unwrap_or(err).trim().to_string()
}

/// Base-owned modules already realized in the reused compile/VM prefix remain
/// resolvable from later live REPL fragments (Issue #11584).
#[test]
fn base_gc_module_survives_live_delta_11584() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("x = 1").success);

        let collected = session.eval("GC.gc()");
        assert!(collected.success, "{:?}", collected.error);
        assert!(matches!(collected.value, None | Some(Value::Nothing)));
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "GC.gc() must resolve against the reused live prefix"
        );
    });
}

/// Carrying Base module metadata must not create a bare Main binding for a
/// non-exported Base submodule (Issue #11584 negative control).
#[test]
fn base_internal_module_stays_hidden_from_live_delta_11584() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("x = 1").success);

        let hidden = session.eval("JuliaSyntax.ParseError");
        assert!(!hidden.success, "non-exported Base module leaked into Main");
        assert!(
            hidden
                .error
                .as_deref()
                .is_some_and(|error| error.contains("UndefVarError")),
            "{:?}",
            hidden.error
        );
    });
}

// --- Display projection, mirroring `repl_session_fixture_tests.rs` (#8714) so
// --- goldens are shared-shaped with the existing harness. ------------------

fn function_definition_display(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let rest = trimmed.strip_prefix("function ")?;
    let name_end = rest.find('(')?;
    let name = rest[..name_end].trim();
    (!name.is_empty()).then(|| format!("{name} (generic function with 1 method)"))
}

fn suppresses_display(input: &str) -> bool {
    input.trim_end().ends_with(';')
}

fn format_value(value: &Value) -> String {
    match value {
        Value::I64(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Str(v) => format!("\"{v}\""),
        Value::Function(f) => format!("{} (generic function)", f.name),
        other => format!("{other:?}"),
    }
}

fn display_of(result: &REPLResult, input: &str) -> Option<String> {
    if !result.success || suppresses_display(input) {
        return None;
    }
    if let Some(display) = function_definition_display(input) {
        return Some(display);
    }
    if let Some(display) = &result.value_display {
        return Some(display.clone());
    }
    result.value.as_ref().map(format_value)
}

// ---------------------------------------------------------------------------
// Driving the two sides.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Action {
    Eval(String),
    Reset,
}

fn observe(session: &mut REPLSession, action: &Action) -> Observation {
    match action {
        Action::Reset => {
            session.reset();
            Observation::action_marker()
        }
        Action::Eval(input) => {
            let result = session.eval(input);
            // Resolve any `StructRef` result against the session's post-eval struct
            // heap so the type proxy is carrier-independent (Issue #9199 S3).
            let heap = session.get_struct_heap();
            let type_repr = result.value.as_ref().map(|v| julia_type_repr(v, heap));
            Observation {
                success: result.success,
                stdout: result.output.clone(),
                display: display_of(&result, input),
                type_repr,
                error: result.error.clone(),
            }
        }
    }
}

fn new_session() -> REPLSession {
    REPLSession::new(0)
}

/// Drive `actions` through two independent production sessions and assert exact
/// per-step equality. Goldens catch deterministic regressions; this comparison
/// independently catches nondeterministic state or cache behavior.
fn run_deterministic_pairs(label: &str, actions: &[Action]) -> Vec<(Observation, Observation)> {
    let mut side_a = new_session();
    let mut side_b = new_session();
    let mut pairs = Vec::with_capacity(actions.len());

    for (index, action) in actions.iter().enumerate() {
        let obs_a = observe(&mut side_a, action);
        let obs_b = observe(&mut side_b, action);
        assert_eq!(
            obs_a,
            obs_b,
            "{label}: step {} ({action:?}) diverged between independent persistent \
             sessions.\nside A: {obs_a:?}\nside B: {obs_b:?}",
            index + 1
        );
        pairs.push((obs_a, obs_b));
    }
    pairs
}

fn run_persistent(label: &str, actions: &[Action]) -> Vec<Observation> {
    run_deterministic_pairs(label, actions)
        .into_iter()
        .map(|(a, _)| a)
        .collect()
}

fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let handler = std::thread::Builder::new()
        .name("repl-differential-9199".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .unwrap();
    if let Err(e) = handler.join() {
        std::panic::resume_unwind(e);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationSeamValueClass {
    Scalar,
    Array,
    ConcreteStruct,
    ImportedStruct,
    NonReconstructable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationSeamBindingUse {
    Rebind,
    OldValueRhs,
    Untouched,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationSeamDefinitionShape {
    None,
    StructBeforeUse,
    StructAfterUse,
    InterleavedFunctionStruct,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationSeamExecutionPath {
    Live,
    FullFallback,
    ErrorBeforeActivation,
    ErrorAfterActivation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationSeamReplayPrefix {
    Plain,
    Import,
    Enum,
}

#[derive(Clone, Copy)]
enum ActivationSeamExpectation {
    Value(&'static str),
    Error(&'static str),
}

struct ActivationSeamCase {
    label: &'static str,
    setup: &'static [&'static str],
    input: &'static str,
    later: &'static str,
    immediate: ActivationSeamExpectation,
    later_display: &'static str,
    value_class: ActivationSeamValueClass,
    binding_use: ActivationSeamBindingUse,
    definition_shape: ActivationSeamDefinitionShape,
    execution_path: ActivationSeamExecutionPath,
    replay_prefix: ActivationSeamReplayPrefix,
}

fn assert_activation_seam_coverage_11564(cases: &[ActivationSeamCase]) {
    let missing = |present: bool, dimension: &str| {
        assert!(present, "activation seam matrix misses {dimension}");
    };
    for value in [
        ActivationSeamValueClass::Scalar,
        ActivationSeamValueClass::Array,
        ActivationSeamValueClass::ConcreteStruct,
        ActivationSeamValueClass::ImportedStruct,
        ActivationSeamValueClass::NonReconstructable,
    ] {
        missing(
            cases.iter().any(|case| case.value_class == value),
            &format!("value class {value:?}"),
        );
    }
    for value in [
        ActivationSeamBindingUse::Rebind,
        ActivationSeamBindingUse::OldValueRhs,
        ActivationSeamBindingUse::Untouched,
    ] {
        missing(
            cases.iter().any(|case| case.binding_use == value),
            &format!("binding use {value:?}"),
        );
    }
    for value in [
        ActivationSeamDefinitionShape::None,
        ActivationSeamDefinitionShape::StructBeforeUse,
        ActivationSeamDefinitionShape::StructAfterUse,
        ActivationSeamDefinitionShape::InterleavedFunctionStruct,
    ] {
        missing(
            cases.iter().any(|case| case.definition_shape == value),
            &format!("definition shape {value:?}"),
        );
    }
    for value in [
        ActivationSeamExecutionPath::Live,
        ActivationSeamExecutionPath::FullFallback,
        ActivationSeamExecutionPath::ErrorBeforeActivation,
        ActivationSeamExecutionPath::ErrorAfterActivation,
    ] {
        missing(
            cases.iter().any(|case| case.execution_path == value),
            &format!("execution path {value:?}"),
        );
    }
    for value in [
        ActivationSeamReplayPrefix::Plain,
        ActivationSeamReplayPrefix::Import,
        ActivationSeamReplayPrefix::Enum,
    ] {
        missing(
            cases.iter().any(|case| case.replay_prefix == value),
            &format!("replay prefix {value:?}"),
        );
    }
}

#[test]
fn activation_seam_matrix_11564() {
    run_with_large_stack(|| {
        let cases = [
            ActivationSeamCase {
                label: "scalar live old-value RHS",
                setup: &["scalar11564 = 40"],
                input: "scalar11564 = scalar11564 + 1",
                later: "scalar11564 + 1",
                immediate: ActivationSeamExpectation::Value("41"),
                later_display: "42",
                value_class: ActivationSeamValueClass::Scalar,
                binding_use: ActivationSeamBindingUse::OldValueRhs,
                definition_shape: ActivationSeamDefinitionShape::None,
                execution_path: ActivationSeamExecutionPath::Live,
                replay_prefix: ActivationSeamReplayPrefix::Plain,
            },
            ActivationSeamCase {
                label: "array rebind after struct-before-use",
                setup: &["array11564 = [1, 2]"],
                input: "macro force_array11564(); :(1); end; struct ArrayFence11564; x::Int; end; array11564 = [41, 1]; sum(array11564)",
                later: "sum(array11564)",
                immediate: ActivationSeamExpectation::Value("42"),
                later_display: "42",
                value_class: ActivationSeamValueClass::Array,
                binding_use: ActivationSeamBindingUse::Rebind,
                definition_shape: ActivationSeamDefinitionShape::StructBeforeUse,
                execution_path: ActivationSeamExecutionPath::FullFallback,
                replay_prefix: ActivationSeamReplayPrefix::Plain,
            },
            // The full merge places current structs before accumulated prior
            // structs. Before #11547, the current marker therefore made the
            // prior `PriorBox11564` constructor private too, and reconstructing
            // `box11564` before user main never reached this old-value RHS.
            ActivationSeamCase {
                label: "concrete struct old-value RHS before struct-after-use",
                setup: &[
                    "struct PriorBox11564; x::Int; end",
                    "box11564 = PriorBox11564(40)",
                ],
                input: "macro force_box11564(); :(1); end; box11564 = PriorBox11564(box11564.x + 1); struct AfterBoxFence11564; x::Int; end; box11564.x",
                later: "box11564.x",
                immediate: ActivationSeamExpectation::Value("41"),
                later_display: "41",
                value_class: ActivationSeamValueClass::ConcreteStruct,
                binding_use: ActivationSeamBindingUse::OldValueRhs,
                definition_shape: ActivationSeamDefinitionShape::StructAfterUse,
                execution_path: ActivationSeamExecutionPath::FullFallback,
                replay_prefix: ActivationSeamReplayPrefix::Plain,
            },
            ActivationSeamCase {
                label: "import replay before untouched package struct",
                setup: &[
                    "using LinearAlgebra",
                    "diag11564 = Diagonal([20, 22])",
                ],
                input: "macro force_diag11564(); :(1); end; diag11564[1, 1] + diag11564[2, 2]",
                later: "sum(diag11564.diag)",
                immediate: ActivationSeamExpectation::Value("42"),
                later_display: "42",
                value_class: ActivationSeamValueClass::ImportedStruct,
                binding_use: ActivationSeamBindingUse::Untouched,
                definition_shape: ActivationSeamDefinitionShape::None,
                execution_path: ActivationSeamExecutionPath::FullFallback,
                replay_prefix: ActivationSeamReplayPrefix::Import,
            },
            ActivationSeamCase {
                label: "non-reconstructable rebind across interleaved definitions",
                setup: &[
                    "struct PairHolder11564; data; tag; end",
                    "makeholder11564(; kw...) = PairHolder11564(kw, 9)",
                    "holder11564 = makeholder11564()",
                ],
                input: "macro force_holder11564(); :(1); end; function holder_before11564(x); x + 1; end; struct HolderFence11564; x::Int; end; holder11564 = makeholder11564(); function holder_after11564(x); HolderFence11564(x); end; holder11564.tag + holder_before11564(32) + holder_after11564(1).x",
                later: "holder11564.tag",
                immediate: ActivationSeamExpectation::Value("43"),
                later_display: "9",
                value_class: ActivationSeamValueClass::NonReconstructable,
                binding_use: ActivationSeamBindingUse::Rebind,
                definition_shape: ActivationSeamDefinitionShape::InterleavedFunctionStruct,
                execution_path: ActivationSeamExecutionPath::FullFallback,
                replay_prefix: ActivationSeamReplayPrefix::Plain,
            },
            ActivationSeamCase {
                label: "catchable error before struct activation",
                setup: &[
                    "struct ErrorBox11564; x::Int; end",
                    "error_box11564 = ErrorBox11564(40)",
                ],
                input: "error(\"before activation 11564\"); struct UnreachedFence11564; x::Int; end",
                later: "error_box11564.x + (isdefined(Main, :UnreachedFence11564) ? 100 : 0)",
                immediate: ActivationSeamExpectation::Error("before activation 11564"),
                later_display: "40",
                value_class: ActivationSeamValueClass::ConcreteStruct,
                binding_use: ActivationSeamBindingUse::Untouched,
                definition_shape: ActivationSeamDefinitionShape::StructAfterUse,
                execution_path: ActivationSeamExecutionPath::ErrorBeforeActivation,
                replay_prefix: ActivationSeamReplayPrefix::Plain,
            },
            ActivationSeamCase {
                label: "catchable error after struct activation",
                setup: &[
                    "struct ErrorAfterBox11564; x::Int; end",
                    "after_box11564 = ErrorAfterBox11564(40)",
                ],
                input: "struct ReachedFence11564; x::Int; end; global after_box11564 = ErrorAfterBox11564(after_box11564.x + 1); error(\"after activation 11564\")",
                later: "after_box11564.x + (isdefined(Main, :ReachedFence11564) ? 1 : 0)",
                immediate: ActivationSeamExpectation::Error("after activation 11564"),
                later_display: "42",
                value_class: ActivationSeamValueClass::ConcreteStruct,
                binding_use: ActivationSeamBindingUse::OldValueRhs,
                definition_shape: ActivationSeamDefinitionShape::StructBeforeUse,
                execution_path: ActivationSeamExecutionPath::ErrorAfterActivation,
                replay_prefix: ActivationSeamReplayPrefix::Plain,
            },
            ActivationSeamCase {
                label: "enum replay before interleaved definitions",
                setup: &[
                    "@enum ReplayEnum11564 enum_left11564=20 enum_right11564=22",
                    "enum_values11564 = [enum_left11564, enum_right11564]",
                ],
                input: "macro force_enum11564(); :(1); end; function enum_before11564(x); x + 1; end; struct EnumFence11564; x::Int; end; function enum_after11564(x); EnumFence11564(x); end; Int(enum_values11564[1]) + Int(enum_values11564[2]) + enum_before11564(0) + enum_after11564(1).x",
                later: "Int(enum_values11564[2])",
                immediate: ActivationSeamExpectation::Value("44"),
                later_display: "22",
                value_class: ActivationSeamValueClass::Array,
                binding_use: ActivationSeamBindingUse::Untouched,
                definition_shape: ActivationSeamDefinitionShape::InterleavedFunctionStruct,
                execution_path: ActivationSeamExecutionPath::FullFallback,
                replay_prefix: ActivationSeamReplayPrefix::Enum,
            },
        ];

        assert_activation_seam_coverage_11564(&cases);
        for case in cases {
            let mut side_a = new_session();
            let mut side_b = new_session();
            for setup in case.setup {
                let action = Action::Eval((*setup).to_string());
                let observed_a = observe(&mut side_a, &action);
                let observed_b = observe(&mut side_b, &action);
                assert_eq!(observed_a, observed_b, "{} setup: {setup}", case.label);
                assert!(
                    observed_a.success,
                    "{} setup: {:?}",
                    case.label, observed_a.error
                );
            }

            let input = Action::Eval(case.input.to_string());
            let immediate_a = observe(&mut side_a, &input);
            let immediate_b = observe(&mut side_b, &input);
            assert_eq!(immediate_a, immediate_b, "{} immediate", case.label);
            match case.immediate {
                ActivationSeamExpectation::Value(display) => {
                    assert!(
                        immediate_a.success,
                        "{}: {:?}",
                        case.label, immediate_a.error
                    );
                    assert_eq!(
                        immediate_a.display.as_deref(),
                        Some(display),
                        "{}",
                        case.label
                    );
                }
                ActivationSeamExpectation::Error(fragment) => {
                    assert!(
                        !immediate_a.success,
                        "{} unexpectedly succeeded",
                        case.label
                    );
                    assert!(
                        immediate_a
                            .error
                            .as_deref()
                            .is_some_and(|error| error.contains(fragment)),
                        "{}: {:?}",
                        case.label,
                        immediate_a.error
                    );
                }
            }
            match case.execution_path {
                ActivationSeamExecutionPath::Live => {
                    assert_eq!(side_a.last_vm_build_nanos(), Some(0), "{}", case.label);
                    assert_eq!(side_b.last_vm_build_nanos(), Some(0), "{}", case.label);
                }
                ActivationSeamExecutionPath::FullFallback => {
                    assert!(
                        side_a.last_vm_build_nanos().is_some_and(|nanos| nanos > 0),
                        "{} did not force a full fallback",
                        case.label
                    );
                    assert!(side_b.last_vm_build_nanos().is_some_and(|nanos| nanos > 0));
                }
                ActivationSeamExecutionPath::ErrorBeforeActivation
                | ActivationSeamExecutionPath::ErrorAfterActivation => {
                    assert!(
                        side_a.has_live_vm(),
                        "{} did not recover its VM",
                        case.label
                    );
                    assert!(
                        side_b.has_live_vm(),
                        "{} did not recover its VM",
                        case.label
                    );
                }
            }

            let later = Action::Eval(case.later.to_string());
            let later_a = observe(&mut side_a, &later);
            let later_b = observe(&mut side_b, &later);
            assert_eq!(later_a, later_b, "{} later", case.label);
            assert!(later_a.success, "{} later: {:?}", case.label, later_a.error);
            assert_eq!(
                later_a.display.as_deref(),
                Some(case.later_display),
                "{} later",
                case.label
            );
        }
    });
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ---------------------------------------------------------------------------
// Reused corpora: the existing repl_session step fixtures (#8714 & friends).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ReplSessionFixture {
    name: String,
    #[allow(dead_code)]
    description: String,
    steps: Vec<ReplStep>,
}

#[derive(Debug, Deserialize)]
struct ReplStep {
    #[allow(dead_code)]
    name: String,
    input: String,
    success: bool,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    display: Option<String>,
    #[serde(default)]
    error_contains: Option<String>,
}

fn load_repl_session_fixtures() -> Vec<(String, ReplSessionFixture)> {
    let dir = fixtures_root().join("repl_session");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "expected reusable repl_session fixtures");
    paths
        .into_iter()
        .map(|path| {
            let src = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let fixture: ReplSessionFixture =
                toml::from_str(&src).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fixture,
            )
        })
        .collect()
}

/// The existing multi-step REPL corpora are model-stable: the same input sequence
/// through two sessions produces identical observations at every step, and each
/// step still matches its recorded success/stdout/error anchor. This pins the
/// current behavior surface that the eval-model migration (Issue #9199) must
/// preserve.
#[test]
fn differential_reused_repl_session_corpora_9199() {
    run_with_large_stack(|| {
        for (file, fixture) in load_repl_session_fixtures() {
            let label = format!("{file}:{}", fixture.name);
            let actions: Vec<Action> = fixture
                .steps
                .iter()
                .map(|s| Action::Eval(s.input.clone()))
                .collect();

            let observations = run_persistent(&label, &actions);

            // Anchor against the corpus's recorded expectations (reusing its
            // goldens without re-deriving the display formatting) so a change that
            // breaks BOTH sides identically is still caught.
            for (step, obs) in fixture.steps.iter().zip(&observations) {
                assert_eq!(
                    obs.success, step.success,
                    "{label} step '{}': success anchor mismatch (error={:?})",
                    step.name, obs.error
                );
                assert_eq!(
                    obs.stdout, step.stdout,
                    "{label} step '{}': stdout anchor mismatch",
                    step.name
                );
                assert_eq!(
                    obs.display, step.display,
                    "{label} step '{}': display anchor mismatch",
                    step.name
                );
                if !step.success {
                    if let Some(expected) = &step.error_contains {
                        let actual = obs.error.as_deref().unwrap_or_default();
                        assert!(
                            actual.contains(expected),
                            "{label} step '{}': error {actual:?} did not contain {expected:?}",
                            step.name
                        );
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Migration-seam corpus: extended by S3+ as constructs cut over.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DiffCorpus {
    name: String,
    #[allow(dead_code)]
    description: String,
    steps: Vec<DiffStep>,
}

#[derive(Debug, Deserialize)]
struct DiffStep {
    name: String,
    /// "eval" (default) or "reset".
    #[serde(default = "default_action")]
    action: String,
    #[serde(default)]
    input: String,
    // Optional shared goldens (absent = not checked).
    expect_success: Option<bool>,
    expect_stdout: Option<String>,
    expect_display: Option<String>,
    expect_type: Option<String>,
    expect_error_contains: Option<String>,
    /// Historical marker for rows where Persistent deliberately corrected Legacy.
    /// These rows use the `*_persistent` goldens below.
    #[serde(default)]
    models_diverge: bool,
    expect_success_persistent: Option<bool>,
    expect_stdout_persistent: Option<String>,
    expect_display_persistent: Option<String>,
    expect_type_persistent: Option<String>,
    expect_error_contains_persistent: Option<String>,
}

fn default_action() -> String {
    "eval".to_string()
}

fn load_diff_corpus(relative_path: &str) -> DiffCorpus {
    let path = fixtures_root().join(relative_path);
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&src).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

impl DiffStep {
    fn to_action(&self) -> Action {
        match self.action.as_str() {
            "reset" => Action::Reset,
            "eval" => Action::Eval(self.input.clone()),
            other => panic!("step '{}': unknown action {other:?}", self.name),
        }
    }
}

/// Assert one observation against a set of optional goldens.
#[allow(clippy::too_many_arguments)]
fn assert_goldens(
    label: &str,
    obs: &Observation,
    expect_success: Option<bool>,
    expect_stdout: Option<&str>,
    expect_display: Option<&str>,
    expect_type: Option<&str>,
    expect_error_contains: Option<&str>,
) {
    if let Some(expected) = expect_success {
        assert_eq!(
            obs.success, expected,
            "{label}: success mismatch ({:?})",
            obs.error
        );
    }
    if let Some(expected) = expect_stdout {
        assert_eq!(obs.stdout, expected, "{label}: stdout mismatch");
    }
    if let Some(expected) = expect_display {
        assert_eq!(
            obs.display.as_deref(),
            Some(expected),
            "{label}: display mismatch"
        );
    }
    if let Some(expected) = expect_type {
        assert_eq!(
            obs.type_repr.as_deref(),
            Some(expected),
            "{label}: type mismatch"
        );
    }
    if let Some(expected) = expect_error_contains {
        let actual = obs.error.as_deref().unwrap_or_default();
        assert!(
            actual.contains(expected),
            "{label}: error {actual:?} did not contain {expected:?}"
        );
    }
}

/// The migration-seam corpus checked against recorded, upstream-reviewed
/// production goldens. Historical `models_diverge` rows select their Persistent
/// goldens; the Legacy expectations remain fixture history and are not executed.
#[test]
fn differential_migration_seams_corpus_9199() {
    run_with_large_stack(|| {
        let corpus = load_diff_corpus("repl_differential/migration_seams_9199.toml");

        let actions: Vec<Action> = corpus.steps.iter().map(DiffStep::to_action).collect();
        let observations = run_persistent(&corpus.name, &actions);

        // Dump ALL observations first (so one REPL_DIFF_DUMP=1 run shows the whole
        // corpus even if a golden below is stale), then assert.
        if std::env::var("REPL_DIFF_DUMP").is_ok() {
            for (step, obs) in corpus.steps.iter().zip(&observations) {
                if step.action != "eval" {
                    continue;
                }
                eprintln!(
                    "STEP {:>36}  persistent=[{}/{:?}/{:?}] err={:?}",
                    step.name,
                    obs.success,
                    obs.display,
                    obs.type_repr,
                    obs.error.as_deref().map(classify_error),
                );
            }
        }

        for (step, obs) in corpus.steps.iter().zip(&observations) {
            if step.action == "reset" {
                continue;
            }
            let label = format!("{}:{}", corpus.name, step.name);
            assert_goldens(
                &label,
                obs,
                if step.models_diverge {
                    step.expect_success_persistent
                } else {
                    step.expect_success
                },
                if step.models_diverge {
                    step.expect_stdout_persistent
                        .as_deref()
                        .or(step.expect_stdout.as_deref())
                } else {
                    step.expect_stdout.as_deref()
                },
                if step.models_diverge {
                    step.expect_display_persistent.as_deref()
                } else {
                    step.expect_display.as_deref()
                },
                if step.models_diverge {
                    step.expect_type_persistent.as_deref()
                } else {
                    step.expect_type.as_deref()
                },
                if step.models_diverge {
                    step.expect_error_contains_persistent.as_deref()
                } else {
                    step.expect_error_contains.as_deref()
                },
            );
        }
    });
}

/// Issue #9793: value-carried Tuple/NamedTuple/Pair globals assigned in one eval
/// and consumed in later evals must match the recorded upstream-reviewed
/// observations. This is the broad corpus guard for the
/// #9786 class, where a runtime-carried NamedTuple only failed under Persistent
/// because VM/container helper arms handled `Value::Tuple` but not the same-shape
/// value carrier.
#[test]
fn differential_value_carried_container_ops_corpus_9793() {
    run_with_large_stack(|| {
        let corpus = load_diff_corpus("repl_differential/value_carried_containers_9793.toml");
        let actions: Vec<Action> = corpus.steps.iter().map(DiffStep::to_action).collect();
        let observations = run_persistent(&corpus.name, &actions);

        if std::env::var("REPL_DIFF_DUMP").is_ok() {
            for (step, obs) in corpus.steps.iter().zip(&observations) {
                if step.action != "eval" {
                    continue;
                }
                eprintln!(
                    "STEP {:>36}  obs=[{}/{:?}/{:?}] err={:?}",
                    step.name,
                    obs.success,
                    obs.display,
                    obs.type_repr,
                    obs.error.as_deref().map(classify_error),
                );
            }
        }

        for (step, obs) in corpus.steps.iter().zip(&observations) {
            if step.action == "reset" {
                continue;
            }
            let label = format!("{}:{}", corpus.name, step.name);
            assert_goldens(
                &label,
                obs,
                step.expect_success,
                step.expect_stdout.as_deref(),
                step.expect_display.as_deref(),
                step.expect_type.as_deref(),
                step.expect_error_contains.as_deref(),
            );
        }
    });
}

/// Issue #9784: an unhandled runtime error unwinds only the current toplevel
/// transaction. Frame 0 remains live, and the following expression re-enters
/// that same VM instead of reconstructing a fresh one.
#[test]
fn runtime_error_recovers_same_live_vm_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("recover_seed9784 = 41").success);

        let failed = session.eval("println(\"before recover error\"); error(\"recover live VM\")");
        assert!(!failed.success, "runtime error must be reported");
        assert_eq!(failed.output, "before recover error\n");
        assert!(
            failed
                .error
                .as_deref()
                .is_some_and(|e| e.contains("recover live VM")),
            "the pre-recovery error must be preserved: {:?}",
            failed.error
        );
        assert!(
            session.has_live_vm(),
            "a runtime error must preserve the recovered live VM"
        );

        let resumed = session.eval("recover_seed9784 + 1");
        assert!(resumed.success, "{:?}", resumed.error);
        assert!(matches!(resumed.value, Some(Value::I64(42))));
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "the first input after an error must reuse the recovered VM"
        );
    });
}

/// Issue #11569 / #9784: a non-shadowing hard scope is an ordinary live
/// toplevel transaction. Its lexical binding must not become a module global,
/// and the same VM must remain parked after the input succeeds.
#[test]
fn hard_scope_nonshadowing_let_reuses_live_vm_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("hard_scope_seed11569 = 40").success);

        let result = session.eval(
            "let hard_scope_local11569 = hard_scope_seed11569 + 1\n  hard_scope_local11569 + 1\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(42))));
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "a hard-scope delta must re-enter the parked VM"
        );
        assert!(session.has_live_vm());

        let leaked = session.eval("isdefined(Main, :hard_scope_local11569)");
        assert!(leaked.success, "{:?}", leaked.error);
        assert!(matches!(leaked.value, Some(Value::Bool(false))));
    });
}

/// Issue #11569: lexical shadowing must use a binding distinct from frame 0.
/// A function compiled outside the `let` therefore reads the module global
/// while the body reads the lexical shadow.
#[test]
fn hard_scope_called_function_reads_global_not_lexical_shadow_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("hard_scope_global11569 = 1").success);
        assert!(
            session
                .eval("hard_scope_reader11569() = hard_scope_global11569")
                .success
        );

        let result = session.eval(
            "let hard_scope_global11569 = 2\n  10 * hard_scope_global11569 + hard_scope_reader11569()\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(21))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert!(session.has_live_vm());

        let global = session.eval("hard_scope_global11569");
        assert!(global.success, "{:?}", global.error);
        assert!(matches!(global.value, Some(Value::I64(1))));
    });
}

/// Issue #11569: nested declarations of the same source name have distinct
/// lexical owners. Exiting the inner scope restores the outer lexical binding,
/// while an independently compiled function continues to see frame 0.
#[test]
fn hard_scope_nested_same_name_uses_innermost_owner_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("nested_scope_global11569 = 1").success);
        assert!(
            session
                .eval("nested_scope_reader11569() = nested_scope_global11569")
                .success
        );

        let result = session.eval(
            "let nested_scope_global11569 = 2\n  inner_scope_value11569 = let nested_scope_global11569 = 3\n    100 * nested_scope_global11569 + nested_scope_reader11569()\n  end\n  inner_scope_value11569 + 10 * nested_scope_global11569\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(321))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let global = session.eval("nested_scope_global11569");
        assert!(global.success, "{:?}", global.error);
        assert!(matches!(global.value, Some(Value::I64(1))));
    });
}

/// Issue #11569 / #9784: a catchable error unwinds transient lexical owners,
/// but keeps completed explicit-global effects and parks the recovered VM.
#[test]
fn hard_scope_error_commits_global_discards_local_and_parks_vm_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("hard_scope_committed11569 = 1").success);

        let failed = session.eval(
            "let hard_scope_transient11569 = 7\n  global hard_scope_committed11569 = 41\n  error(\"hard scope transaction 11569\")\nend",
        );
        assert!(!failed.success, "the hard-scope error must be reported");
        assert!(failed
            .error
            .as_deref()
            .is_some_and(|error| error.contains("hard scope transaction 11569")));
        assert!(
            session.has_live_vm(),
            "a catchable hard-scope error must leave the recovered VM parked"
        );
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let committed = session.eval("hard_scope_committed11569 + 1");
        assert!(committed.success, "{:?}", committed.error);
        assert!(matches!(committed.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let transient = session.eval("isdefined(Main, :hard_scope_transient11569)");
        assert!(transient.success, "{:?}", transient.error);
        assert!(matches!(transient.value, Some(Value::Bool(false))));
    });
}

/// Issue #11569: a top-level loop binder is lexical even though the enclosing
/// REPL input runs against frame 0. Called functions must keep seeing the module
/// global, and a `break` must unwind the binder before the VM is parked.
#[test]
fn hard_scope_loop_binder_is_lexical_across_call_and_break_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("loop_scope_global11569 = 1").success);
        assert!(
            session
                .eval("loop_scope_reader11569() = loop_scope_global11569")
                .success
        );
        assert!(session.eval("loop_scope_observed11569 = 0").success);

        let loop_result = session.eval(
            "for loop_scope_global11569 in 2:3\n  global loop_scope_observed11569 = 10 * loop_scope_global11569 + loop_scope_reader11569()\n  break\nend",
        );
        assert!(loop_result.success, "{:?}", loop_result.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert!(session.has_live_vm());

        let observed = session.eval("loop_scope_observed11569");
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::I64(21))));
        let global = session.eval("loop_scope_global11569");
        assert!(global.success, "{:?}", global.error);
        assert!(matches!(global.value, Some(Value::I64(1))));
    });
}

/// Issue #11569: an error caught outside a loop crosses the loop's lexical
/// owner. The binder must be removed before catch resumes, without changing the
/// same-named global read by an independently compiled function.
#[test]
fn hard_scope_loop_binder_unwinds_before_outer_catch_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("loop_catch_global11569 = 1").success);
        assert!(
            session
                .eval("loop_catch_reader11569() = loop_catch_global11569")
                .success
        );

        let result = session.eval(
            "try\n  for loop_catch_global11569 in 2:2\n    error(\"loop catch 11569\")\n  end\ncatch\nend\n10 * loop_catch_reader11569() + loop_catch_global11569",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(11))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert!(session.has_live_vm());
    });
}

/// Issue #11569: a struct value held by a live lexical owner round-trips through
/// the explicit environment. VM-level tests force collection and pin remapping;
/// the public REPL regression stays independent of the separate `GC` live-delta
/// gap tracked by Issue #11584.
#[test]
fn hard_scope_lexical_struct_value_round_trips_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definition = session.eval("struct LexicalGcBox11569\n  value::Int\nend");
        assert!(definition.success, "{:?}", definition.error);

        let result = session.eval(
            "let lexical_gc_box11569 = LexicalGcBox11569(41)\n  lexical_gc_box11569.value + 1\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert!(session.has_live_vm());
    });
}

/// Issue #11569: explicit lexical bytecode is also required by the first full
/// user-main compile, not only by seeded deltas. This is the canonical one-shot
/// MWE from the Issue reduced to a scalar assertion.
#[test]
fn hard_scope_one_shot_full_compile_keeps_global_identity_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "one_shot_global11569 = 1; one_shot_reader11569() = one_shot_global11569; one_shot_observed11569 = let one_shot_global11569 = 2\n  10 * one_shot_global11569 + one_shot_reader11569()\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(session.has_live_vm());

        // A definition-bearing REPL input displays the generic function, so
        // observe both values in the next delta while retaining the one-shot
        // compile of the lexical body above.
        let observed = session.eval("100 * one_shot_observed11569 + one_shot_global11569");
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::I64(2_101))));
    });
}

/// Issue #11569: Julia evaluates each explicit `let` RHS in the enclosing
/// environment and activates bindings sequentially. An uninitialized owner is
/// nevertheless present and must suppress fallback to a same-named global.
#[test]
fn hard_scope_sequential_and_uninitialized_owners_match_julia_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("sequential_global11569 = 9").success);

        let outer_rhs = session.eval(
            "let sequential_global11569 = sequential_global11569\n  sequential_global11569\nend",
        );
        assert!(outer_rhs.success, "{:?}", outer_rhs.error);
        assert!(matches!(outer_rhs.value, Some(Value::I64(9))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let sequential = session.eval(
            "let sequential_a11569 = 1, sequential_b11569 = sequential_a11569\n  10 * sequential_a11569 + sequential_b11569\nend",
        );
        assert!(sequential.success, "{:?}", sequential.error);
        assert!(matches!(sequential.value, Some(Value::I64(11))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        assert!(session.eval("uninitialized_global11569 = 1").success);
        let is_defined = session
            .eval("let uninitialized_global11569\n  @isdefined(uninitialized_global11569)\nend");
        assert!(is_defined.success, "{:?}", is_defined.error);
        assert!(matches!(is_defined.value, Some(Value::Bool(false))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let read = session.eval("let uninitialized_global11569\n  uninitialized_global11569\nend");
        assert!(
            !read.success,
            "an undefined lexical owner must not read frame 0"
        );
        assert!(read
            .error
            .as_deref()
            .is_some_and(|error| error.contains("UndefVarError")));
        assert!(session.has_live_vm());

        assert!(session.eval("local_decl_global11569 = 1").success);
        let local_decl = session
            .eval("let\n  local local_decl_global11569\n  @isdefined(local_decl_global11569)\nend");
        assert!(local_decl.success, "{:?}", local_decl.error);
        assert!(matches!(local_decl.value, Some(Value::Bool(false))));
    });
}

/// Issue #11569: a comprehension binder owns a lexical name exactly like a
/// `for` binder. The independently compiled reader must see frame 0 while the
/// element expression is running, and the binder must not leak afterward.
#[test]
fn hard_scope_comprehension_binder_is_lexical_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("comprehension_global11569 = 1").success);
        assert!(
            session
                .eval("comprehension_reader11569() = comprehension_global11569")
                .success
        );

        let result = session.eval(
            "let\n  comprehension_values11569 = [comprehension_reader11569() for comprehension_global11569 in 2:2]\n  comprehension_values11569[1]\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(1))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let global = session.eval("comprehension_global11569");
        assert!(global.success, "{:?}", global.error);
        assert!(matches!(global.value, Some(Value::I64(1))));
    });
}

/// Issue #11569: every comprehension form owns its binders outside frame 0.
/// Tuple destructuring, comma/cartesian, and dependent flatten forms must all
/// keep independently compiled readers on the module globals.
#[test]
fn hard_scope_all_comprehension_binders_preserve_global_identity_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();

        assert!(session.eval("tuple_comp_global11569 = 100").success);
        assert!(
            session
                .eval("tuple_comp_reader11569() = tuple_comp_global11569")
                .success
        );
        let tuple = session.eval(
            "let values11569 = [10 * tuple_comp_global11569 + tuple_comp_reader11569() for (tuple_comp_global11569, tuple_comp_other11569) in [(2, 3)]]\n  values11569[1]\nend",
        );
        assert!(tuple.success, "{:?}", tuple.error);
        assert!(matches!(tuple.value, Some(Value::I64(120))));

        assert!(session.eval("multi_comp_global11569 = 100").success);
        assert!(
            session
                .eval("multi_comp_reader11569() = multi_comp_global11569")
                .success
        );
        let cartesian = session.eval(
            "let values11569 = [10 * multi_comp_global11569 + multi_comp_reader11569() for multi_comp_global11569 in 2:2, multi_comp_other11569 in 1:1]\n  values11569[1]\nend",
        );
        assert!(cartesian.success, "{:?}", cartesian.error);
        assert!(matches!(cartesian.value, Some(Value::I64(120))));

        let flatten = session.eval(
            "let values11569 = [10 * multi_comp_global11569 + multi_comp_reader11569() + dependent11569 for multi_comp_global11569 in 2:2 for dependent11569 in multi_comp_global11569:multi_comp_global11569]\n  values11569[1]\nend",
        );
        assert!(flatten.success, "{:?}", flatten.error);
        assert!(matches!(flatten.value, Some(Value::I64(122))));

        let globals = session.eval(
            "tuple_comp_global11569 == 100 && multi_comp_global11569 == 100 && !isdefined(Main, :tuple_comp_other11569) && !isdefined(Main, :multi_comp_other11569) && !isdefined(Main, :dependent11569)",
        );
        assert!(globals.success, "{:?}", globals.error);
        assert!(matches!(globals.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: a `global x` declaration belongs to its lexical scope and must
/// not poison a nested explicit let/comprehension owner with the same name.
#[test]
fn hard_scope_nested_owner_shadows_outer_global_declaration_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("outer_declared_global11569 = 100").success);
        let result = session.eval(
            "let_observed11569 = let\n  global outer_declared_global11569\n  let outer_declared_global11569 = 2\n    outer_declared_global11569\n  end\nend\ncomp_observed11569 = let\n  global outer_declared_global11569\n  [outer_declared_global11569 for outer_declared_global11569 in 1:1][1]\nend\nouter_declared_global11569 == 100 && let_observed11569 == 2 && comp_observed11569 == 1",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: mutable values stored in a lexical owner use the same
/// heap-indirected identity representation as ordinary VM slots.
#[test]
fn hard_scope_mutable_struct_alias_identity_is_preserved_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definition = session.eval("mutable struct LexicalMutable11569\n  value::Int\nend");
        assert!(definition.success, "{:?}", definition.error);

        let result = session.eval(
            "let first11569 = LexicalMutable11569(1), alias11569 = first11569\n  first11569.value = 2\n  alias11569.value\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(2))));
    });
}

/// Issue #11569: the catch binder must snapshot the pending exception before
/// `ClearError` transfers it to the caught-exception stack.
#[test]
fn hard_scope_catch_binder_keeps_original_exception_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("catch_binder_observed11569 = false").success);
        let result = session.eval(
            "let\n  try\n    error(\"catch binder 11569\")\n  catch caught11569\n    global catch_binder_observed11569 = caught11569 isa ErrorException\n  end\nend",
        );
        assert!(result.success, "{:?}", result.error);
        let observed = session.eval("catch_binder_observed11569");
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: expression-form `continue`/`break` emitted by short-circuit
/// lowering must close nested lexical owners before jumping to the loop target.
#[test]
fn hard_scope_short_circuit_loop_exits_balance_lexical_owners_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("short_scope_global11569 = 100").success);
        assert!(session.eval("short_scope_count11569 = 0").success);
        assert!(
            session
                .eval("short_scope_reader11569() = short_scope_global11569")
                .success
        );

        let continued = session.eval(
            "for short_scope_global11569 in 1:2\n  global short_scope_count11569 += 1\n  let short_scope_global11569 = 99\n    true && continue\n  end\nend\n10000 * short_scope_count11569 + 10 * short_scope_reader11569() + short_scope_global11569",
        );
        assert!(continued.success, "{:?}", continued.error);
        assert!(matches!(continued.value, Some(Value::I64(21_100))));

        let broken = session.eval(
            "for short_scope_global11569 in 1:2\n  let short_scope_global11569 = 99\n    true && break\n  end\nend\n10 * short_scope_reader11569() + short_scope_global11569",
        );
        assert!(broken.success, "{:?}", broken.error);
        assert!(matches!(broken.value, Some(Value::I64(1_100))));
    });
}

/// Issue #11569: a pending `finally` runs before the non-local loop exit pops
/// the lexical owners whose values the finally body is allowed to read.
#[test]
fn hard_scope_finally_observes_owner_before_break_cleanup_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("finally_scope_global11569 = 1").success);
        assert!(session.eval("finally_scope_observed11569 = 0").success);
        assert!(
            session
                .eval("finally_scope_reader11569() = finally_scope_global11569")
                .success
        );

        let result = session.eval(
            "for finally_scope_index11569 in 1:1\n  let finally_scope_global11569 = 2\n    try\n      break\n    finally\n      global finally_scope_observed11569 = 10 * finally_scope_global11569 + finally_scope_reader11569()\n    end\n  end\nend\n100 * finally_scope_observed11569 + finally_scope_global11569",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(2_101))));
    });
}

/// Issue #11569: return runs pending finally code before closing the lexical
/// owners that enclose the transfer, symmetric with break/continue.
#[test]
fn hard_scope_finally_observes_owner_before_return_cleanup_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "let return_visible11569 = 1\n  try\n    return 2\n  finally\n    println(return_visible11569)\n  end\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output, "1\n");
        assert!(matches!(result.value, Some(Value::I64(2))));
    });
}

/// Issue #11569: expression-form return (`cond && return`) must share the
/// statement-return transfer path, including pending finally execution.
#[test]
fn hard_scope_finally_runs_for_short_circuit_return_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "return_flag11569 = true\nlet return_expr_visible11569 = 7\n  try\n    return_flag11569 && return 2\n  finally\n    println(return_expr_visible11569)\n  end\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output, "7\n");
        assert!(matches!(result.value, Some(Value::I64(2))));
    });
}

/// Issue #11569: assignments in every comprehension body/filter form are
/// comprehension locals unless that expression explicitly declares `global`.
#[test]
fn hard_scope_comprehension_assignment_owners_preserve_globals_11569() {
    run_with_large_stack(|| {
        let cases = [
            "comp_owner11569 = 100; a = [(comp_owner11569 = 1) for i in 1:1]; 1000 * comp_owner11569 + a[1]",
            "comp_owner11569 = 100; a = [(comp_owner11569 = 1) for (i, j) in [(1, 2)]]; 1000 * comp_owner11569 + a[1]",
            "comp_owner11569 = 100; a = [(comp_owner11569 = 1) for i in 1:1, j in 1:1]; 1000 * comp_owner11569 + a[1]",
            "comp_owner11569 = 100; a = [(comp_owner11569 = 1) for i in 1:1 for j in 1:1]; 1000 * comp_owner11569 + a[1]",
            "comp_owner11569 = 100; a = [i for i in 1:1 if (comp_owner11569 = 1) == 1]; 1000 * comp_owner11569 + a[1]",
            "comp_owner11569 = 100; a = [(i, j) for i in 1:1 for j in (comp_owner11569 = 1):1]; 1000 * comp_owner11569 + a[1][1]",
        ];
        for source in cases {
            let mut session = new_session();
            let result = session.eval(source);
            assert!(result.success, "source={source}\nerror={:?}", result.error);
            assert!(
                matches!(result.value, Some(Value::I64(100_001))),
                "source={source}\nvalue={:?}",
                result.value
            );
        }

        let mut session = new_session();
        let explicit_global = session.eval(
            "comp_global11569 = 100; a = [global comp_global11569 = 1 for i in 1:1]; 1000 * comp_global11569 + a[1]",
        );
        assert!(explicit_global.success, "{:?}", explicit_global.error);
        assert!(matches!(explicit_global.value, Some(Value::I64(1_001))));
    });
}

/// Issue #11569: nested loops and try clauses own their own locals. The outer
/// `let` must not predeclare names found only inside those nested scopes.
#[test]
fn hard_scope_nested_scope_locals_do_not_escape_to_outer_let_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "let\n  for nested_loop_index11569 in 1:1\n    nested_loop_only11569 = 2\n  end\n  try\n    nested_try_only11569 = 3\n  catch\n  end\n  !@isdefined(nested_loop_only11569) && !@isdefined(nested_try_only11569)\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: a soft-scope assignment updates an existing outer lexical
/// owner or interactive global; only a genuinely new/explicit local receives a
/// fresh loop-body owner.
#[test]
fn hard_scope_loop_assignment_reuses_existing_owner_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();

        let lexical = session.eval(
            "let outer_counter11569 = 0\n  for owner_index11569 in 1:2\n    outer_counter11569 += 1\n  end\n  outer_counter11569\nend",
        );
        assert!(lexical.success, "{:?}", lexical.error);
        assert!(matches!(lexical.value, Some(Value::I64(2))));

        assert!(session.eval("interactive_counter11569 = 0").success);
        let loop_result =
            session.eval("for owner_index11569 in 1:2\n  interactive_counter11569 += 1\nend");
        assert!(loop_result.success, "{:?}", loop_result.error);
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "updating a pre-existing interactive global must use the live delta"
        );
        let global = session.eval("interactive_counter11569");
        assert!(global.success, "{:?}", global.error);
        assert!(matches!(global.value, Some(Value::I64(2))));

        assert!(session.eval("try_counter11569 = 0").success);
        let try_result = session.eval("try\n  try_counter11569 += 1\ncatch\nend");
        assert!(try_result.success, "{:?}", try_result.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        let try_global = session.eval("try_counter11569");
        assert!(try_global.success, "{:?}", try_global.error);
        assert!(matches!(try_global.value, Some(Value::I64(1))));

        assert!(session.eval("nested_counter11569 = 0").success);
        let nested = session.eval(
            "let\n  for nested_owner_index11569 in 1:1\n    nested_counter11569 += 1\n  end\nend",
        );
        assert!(
            !nested.success,
            "a loop nested in a hard scope must create an uninitialized local"
        );
        assert!(nested
            .error
            .as_deref()
            .is_some_and(|error| error.contains("UndefVarError")));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        let nested_global = session.eval("nested_counter11569");
        assert!(nested_global.success, "{:?}", nested_global.error);
        assert!(matches!(nested_global.value, Some(Value::I64(0))));
    });
}

/// Issue #11569: module bodies use the same explicit lexical environment as
/// Main, so a method defined outside the `let` keeps reading the module global.
#[test]
fn hard_scope_module_body_keeps_global_identity_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "module LexicalModule11569\n  module_global11569 = 1\n  module_reader11569() = module_global11569\n  observed11569 = let module_global11569 = 2\n    10 * module_global11569 + module_reader11569()\n  end\nend\n100 * LexicalModule11569.observed11569 + LexicalModule11569.module_global11569",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(2_101))));
    });
}

/// Issue #11569: a clause-local declaration shadows a same-named module
/// constant without mutating the qualified module binding.
#[test]
fn hard_scope_module_clause_local_shadows_constant_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "module LexicalConstModule11569\n  const constant11569 = 1\n  observed11569 = -1\n  try\n    local constant11569 = 2\n    global observed11569 = constant11569\n  catch\n  end\nend\n100 * LexicalConstModule11569.observed11569 + LexicalConstModule11569.constant11569",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(201))));
    });
}

/// Issue #11569: the catch binder's compiler metadata is clause-local. After
/// the clause exits, a same-named module constant must resolve to the qualified
/// module binding rather than the dead lexical owner.
#[test]
fn hard_scope_module_catch_binder_does_not_leak_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "module CatchLeakModule11569\n  const caught11569 = 7\n  observed_type11569 = false\n  try\n    error(\"catch leak 11569\")\n  catch caught11569\n    global observed_type11569 = caught11569 isa ErrorException\n  end\n  observed_value11569 = caught11569\nend\nCatchLeakModule11569.observed_type11569 && CatchLeakModule11569.observed_value11569 == 7",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: a lexical owner shadows a same-named module receiver for
/// field assignment just as it does for qualified calls.
#[test]
fn hard_scope_field_assign_prefers_lexical_owner_over_module_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "module FieldOwnerModule11569\n  x = 1\nend\nmutable struct FieldOwnerBox11569\n  x::Int\nend\nfield_owner_value11569 = let FieldOwnerModule11569 = FieldOwnerBox11569(2)\n  FieldOwnerModule11569.x = 3\n  FieldOwnerModule11569.x\nend\n100 * FieldOwnerModule11569.x + field_owner_value11569",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(103))));
    });
}

/// Issue #11569: an explicit clause local shadows an imported module alias at
/// module depth zero. The alias becomes visible again after the clause exits.
#[test]
fn hard_scope_module_clause_local_shadows_import_alias_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "module ClauseAliasModule11569\n  import Random\n  observed11569 = false\n  try\n    local Random = 3\n    global observed11569 = Random == 3\n  catch\n  end\nend\nClauseAliasModule11569.observed11569",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: static type/function alias metadata belongs to the same
/// lexical owner as its runtime value. A clause or let-local alias must not
/// change a later top-level `invoke` signature.
#[test]
fn hard_scope_type_alias_metadata_does_not_leak_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "alias_method11569(x::Int) = 1\nalias_method11569(x::Float64) = 2\nAliasTuple11569 = Tuple{Int}\ntry\n  local AliasTuple11569 = Tuple{Float64}\ncatch\nend\nclause_observed11569 = invoke(alias_method11569, AliasTuple11569, 1)\nlet AliasTuple11569 = Tuple{Float64}\n  nothing\nend\nlet_observed11569 = invoke(alias_method11569, AliasTuple11569, 1)\nclause_observed11569 == 1 && let_observed11569 == 1",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: catching a loop error inside an outer lexical owner must pop
/// only the loop owner. The outer binding remains active, while the called
/// function continues to read the module global.
#[test]
fn hard_scope_inner_loop_error_restores_outer_lexical_owner_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("outer_loop_global11569 = 1").success);
        assert!(
            session
                .eval("outer_loop_reader11569() = outer_loop_global11569")
                .success
        );

        let result = session.eval(
            "let outer_loop_global11569 = 10\n  try\n    for outer_loop_global11569 in 1:1\n      error(\"inner loop owner 11569\")\n    end\n  catch\n  end\n  10 * outer_loop_global11569 + outer_loop_reader11569()\nend",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(101))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Issue #11569: a closure escaping a hard scope snapshots the lexical value
/// under its source capture name. The closure and its `Ref` payload remain live
/// across a later REPL delta. Forced-GC remapping is covered at VM level; the
/// separate live-REPL `GC` module gap is Issue #11584.
#[test]
fn hard_scope_escaping_closure_keeps_lexical_capture_live_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("closure_scope_seed11569 = 1").success);

        let created = session.eval(
            "escaped_scope_closure11569 = let closure_scope_ref11569 = Ref(7)\n  () -> closure_scope_ref11569[]\nend",
        );
        assert!(created.success, "{:?}", created.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert!(session.has_live_vm());

        let resumed = session.eval("escaped_scope_closure11569()");
        assert!(resumed.success, "{:?}", resumed.error);
        assert!(matches!(resumed.value, Some(Value::I64(7))));
    });
}

/// Issue #11569: anonymous helper identity is stable across REPL fragments.
/// A later closure must not replace the body selected by an earlier closure.
#[test]
fn hard_scope_multiple_escaping_closures_keep_distinct_helpers_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("closure_identity_seed11569 = 1").success);

        let first = session.eval("first_closure11569 = let a11569 = 1\n  () -> a11569\nend");
        assert!(first.success, "{:?}", first.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let second = session.eval("second_closure11569 = let b11569 = 2\n  () -> b11569\nend");
        assert!(second.success, "{:?}", second.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let observed = session.eval("first_closure11569() == 1 && second_closure11569() == 2");
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::Bool(true))));
    });
}

/// Issue #11569: a later ineligible input may rebuild the VM from accumulated
/// source. Lifted helper IR must survive that rebuild with its captured closure.
#[test]
fn hard_scope_escaping_closure_survives_full_rebuild_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("closure_rebuild_seed11569 = 1").success);
        let created = session.eval(
            "rebuild_closure11569 = let rebuild_capture11569 = Ref(9)\n  () -> rebuild_capture11569[]\nend",
        );
        assert!(created.success, "{:?}", created.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let rebuilt = session.eval(
            "module ClosureFallbackModule11569\n  const marker11569 = 1\nend\nClosureFallbackModule11569.marker11569",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(session.last_vm_build_nanos().is_some_and(|nanos| nanos > 0));

        let resumed = session.eval("rebuild_closure11569()");
        assert!(resumed.success, "{:?}", resumed.error);
        assert!(matches!(resumed.value, Some(Value::I64(9))));
    });
}

/// Issue #11569: immediate helper installation participates in the same error
/// transaction as source-marked definitions. Mutations and a closure published
/// before a catchable top-level error remain live and compiler-aligned.
#[test]
fn hard_scope_closure_helper_error_recovery_commits_prefix_11569() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("closure_error_global11569 = 0").success);

        let failed = session.eval(
            "closure_error_global11569 = 1\nerror_closure11569 = let error_capture11569 = 7\n  () -> error_capture11569\nend\nerror(\"after closure helper 11569\")",
        );
        assert!(!failed.success, "catchable error must be reported");
        assert!(
            session.has_live_vm(),
            "recoverable live VM must be retained"
        );

        let recovered = session.eval("closure_error_global11569 == 1 && error_closure11569() == 7");
        assert!(recovered.success, "{:?}", recovered.error);
        assert!(matches!(recovered.value, Some(Value::Bool(true))));
    });
}

/// Issue #9784: Julia commits mutations that ran before a later toplevel
/// exception. The recovered runtime state must be synchronized before any
/// remaining full-fallback path reconstructs from the transitional mirror.
#[test]
fn mutation_before_runtime_error_survives_live_and_fallback_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("recover_mut9784 = 1").success);

        let failed = session.eval("recover_mut9784 = 41; error(\"after mutation\")");
        assert!(!failed.success, "runtime error must be reported");
        assert!(
            session.has_live_vm(),
            "the mutated VM must remain authoritative"
        );

        // The hard-scope delta must run against the authoritative recovered VM
        // and see the value committed before the exception.
        let live_hard_scope = session.eval("let local9784 = recover_mut9784\n  local9784 + 1\nend");
        assert!(live_hard_scope.success, "{:?}", live_hard_scope.error);
        assert!(matches!(live_hard_scope.value, Some(Value::I64(42))));
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "hard scope must use the live delta path"
        );
    });
}

/// Issue #9784: an unhandled error inside a redirect thunk must restore the
/// previous stream before the VM is parked. Clearing only the redirect-state
/// stack would leave `current_stdout` pointing at `devnull` in the next eval.
#[test]
fn runtime_error_recovery_restores_redirected_stdout_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definition = session.eval("redirect_boom9784() = error(\"redirect boom\")");
        assert!(definition.success, "{:?}", definition.error);

        let failed = session.eval("redirect_stdout(redirect_boom9784, devnull)");
        assert!(!failed.success, "redirect thunk must report its error");
        assert!(session.has_live_vm(), "the errored VM must be recovered");

        let resumed = session.eval("println(\"visible after redirect error\"); 42");
        assert!(resumed.success, "{:?}", resumed.error);
        assert_eq!(resumed.output, "visible after redirect error\n");
        assert!(matches!(resumed.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// `ans` has a host-facing mirror separate from ordinary REPL globals. A value
/// committed before an error must update both the recovered frame 0 and that
/// mirror, including its compiler-facing type metadata.
#[test]
fn ans_assignment_before_error_updates_host_mirror_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("1").success);

        let failed = session.eval("ans = 41; error(\"ans boom\")");
        assert!(!failed.success, "runtime error must be reported");
        assert!(matches!(session.get_ans(), Some(Value::I64(41))));

        let resumed = session.eval("ans + 1");
        assert!(resumed.success, "{:?}", resumed.error);
        assert!(matches!(resumed.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Issue #9784: a called function may create a brand-new module global before
/// throwing. That binding is absent from both the current main's assignment list
/// and the pre-eval session mirror, but it is committed in frame 0 and must be
/// synchronized before a later full fallback.
#[test]
fn new_function_global_before_error_survives_fallback_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definition = session
            .eval("latent_writer9784() = (global latent_error9784 = 41; error(\"latent boom\"))");
        assert!(definition.success, "{:?}", definition.error);

        let failed = session.eval("latent_writer9784()");
        assert!(!failed.success, "called function must report its error");
        assert!(session.has_live_vm(), "the errored VM must be recovered");

        let live_update = session.eval("latent_error9784 += 1");
        assert!(live_update.success, "{:?}", live_update.error);
        assert!(matches!(live_update.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let forced_fallback =
            session.eval("let local9784 = latent_error9784\n  local9784 + 1\nend");
        assert!(forced_fallback.success, "{:?}", forced_fallback.error);
        assert!(matches!(forced_fallback.value, Some(Value::I64(43))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Issue #9784: once an indirectly-created global has entered the session
/// mirror on a successful live run, a later failing call must refresh that same
/// binding rather than lose the mutation on the next full fallback.
#[test]
fn function_global_success_then_error_survives_fallback_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definition = session.eval(
            "latent_sequence_writer9784(v, fail) = (global latent_sequence9784 = v; fail && error(\"sequence boom\"); v)",
        );
        assert!(definition.success, "{:?}", definition.error);

        let first = session.eval("latent_sequence_writer9784(1, false)");
        assert!(first.success, "{:?}", first.error);
        assert!(matches!(first.value, Some(Value::I64(1))));

        let failed = session.eval("latent_sequence_writer9784(41, true)");
        assert!(!failed.success, "called function must report its error");
        assert!(session.has_live_vm(), "the errored VM must be recovered");

        let forced_fallback =
            session.eval("let local9784 = latent_sequence9784\n  local9784 + 1\nend");
        assert!(forced_fallback.success, "{:?}", forced_fallback.error);
        assert!(matches!(forced_fallback.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Upstream evaluates the children of an `Expr(:toplevel)` in order, so a method
/// definition reached before a later exception remains installed. A live delta
/// must commit the matching compiler snapshot and keep that same VM re-enterable
/// instead of discarding the reached definition (Issue #9784).
#[test]
fn function_definition_is_not_visible_before_source_marker_11477() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let old = session.eval("old_source_order11477() = 0");
        assert!(old.success, "{:?}", old.error);

        let definition = session.eval(
            "observed_source_order11477 = isdefined(Main, :late_source_order11477) ? late_source_order11477() : -1; late_source_order11477() = 41",
        );
        assert!(definition.success, "{:?}", definition.error);
        assert!(session.has_live_vm());

        let observed = session.eval("observed_source_order11477");
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::I64(-1))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

#[test]
fn direct_function_call_is_not_visible_before_source_marker_11477() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let old = session.eval("old_direct_source_order11477() = 0");
        assert!(old.success, "{:?}", old.error);

        let definition = session.eval(
            "observed_direct_source_order11477 = try\n  late_direct_source_order11477()\ncatch\n  -1\nend; late_direct_source_order11477() = 42",
        );
        assert!(definition.success, "{:?}", definition.error);

        let observed = session.eval("observed_direct_source_order11477");
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::I64(-1))));
    });
}

#[test]
fn explicit_global_function_read_is_not_visible_before_source_marker_11655() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definition = session.eval(
            "function read_late_global_function11655()\n  global late_global_function11655\n  late_global_function11655\nend\nhidden_global_function11655 = try\n  read_late_global_function11655()\n  false\ncatch e\n  e isa UndefVarError\nend\nlate_global_function11655() = 43",
        );
        assert!(definition.success, "{:?}", definition.error);

        let observed = session.eval(
            "(hidden_global_function11655, read_late_global_function11655() === late_global_function11655)",
        );
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(
            observed.value,
            Some(Value::Tuple(ref values))
                if matches!(values.elements.as_slice(), [Value::Bool(true), Value::Bool(true)])
        ));
    });
}

/// Lowered anonymous callables are values created by their containing
/// expression, not top-level generic definitions. The source-order fence for a
/// later named method must therefore not leave a same-statement arrow dormant.
#[test]
fn anonymous_callable_is_visible_within_defining_statement_11477() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let result = session.eval(
            "lambda_dict11477 = Dict{Int,Int}(); get!(() -> 42, lambda_dict11477, 3); lambda_dict11477[3]",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(42))));
    });
}

/// Concrete type metadata may be compiled ahead of main for stable `type_id`s,
/// but the Julia binding must not become visible until execution reaches the
/// declaration. A direct constructor call before that point raises the same
/// catchable `UndefVarError` as upstream (Issue #11546).
#[test]
fn struct_binding_activates_at_source_position_11546() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_type_order11546 = 1").success);

        let result = session.eval(
            "before_type11546 = isdefined(Main, :LateType11546); \
             before_ctor11546 = try\n  LateType11546(1); false\ncatch e\n  e isa UndefVarError\nend; \
             struct LateType11546\n  x::Int\nend; \
             (before_type11546, before_ctor11546, isdefined(Main, :LateType11546), LateType11546(42).x)",
        );
        assert!(result.success, "{:?}", result.error);

        let observed = session.eval(
            "before_type11546 == false && before_ctor11546 == true && LateType11546(42).x == 42",
        );
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::Bool(true))));
    });
}

/// Compiler-known nominal metadata is private until execution reaches the
/// declaration. Upstream reports `isdefined == false` and a catchable
/// `UndefVarError` before each marker, then publishes the Main binding at the
/// marker itself (Issues #9784/#11635).
#[test]
fn nominal_type_bindings_activate_at_source_position_11635() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_nominal_order11635 = 1").success);

        let abstract_result = session.eval(
            "before_a11635 = isdefined(Main, :LiveAbstract11635); \
             abstract_hidden11635 = try\n  LiveAbstract11635; false\ncatch e\n  e isa UndefVarError\nend; \
             abstract type LiveAbstract11635 end; \
             after_a11635 = isdefined(Main, :LiveAbstract11635)",
        );
        assert!(abstract_result.success, "{:?}", abstract_result.error);
        let abstract_visibility =
            session.eval("!before_a11635 && abstract_hidden11635 && after_a11635");
        assert!(
            matches!(abstract_visibility.value, Some(Value::Bool(true))),
            "abstract visibility: value={:?}, error={:?}",
            abstract_visibility.value,
            abstract_visibility.error
        );
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let primitive_result = session.eval(
            "before_p11635 = isdefined(Main, :LivePrimitive11635); \
             primitive_hidden11635 = try\n  LivePrimitive11635; false\ncatch e\n  e isa UndefVarError\nend; \
             primitive type LivePrimitive11635 8 end; \
             after_p11635 = isdefined(Main, :LivePrimitive11635)",
        );
        assert!(primitive_result.success, "{:?}", primitive_result.error);
        let primitive_visibility =
            session.eval("!before_p11635 && primitive_hidden11635 && after_p11635");
        assert!(
            matches!(primitive_visibility.value, Some(Value::Bool(true))),
            "primitive visibility: value={:?}, error={:?}",
            primitive_visibility.value,
            primitive_visibility.error
        );
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let enum_result = session.eval(
            "before_e11635 = isdefined(Main, :LiveEnum11635); \
             enum_hidden11635 = try\n  LiveEnum11635; false\ncatch e\n  e isa UndefVarError\nend; \
             @enum LiveEnum11635 live_enum_zero11635 live_enum_one11635; \
             after_e11635 = isdefined(Main, :LiveEnum11635)",
        );
        assert!(enum_result.success, "{:?}", enum_result.error);
        let enum_visibility = session.eval("!before_e11635 && enum_hidden11635 && after_e11635");
        assert!(
            matches!(enum_visibility.value, Some(Value::Bool(true))),
            "enum visibility: value={:?}, error={:?}",
            enum_visibility.value,
            enum_visibility.error
        );
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// `instances(Enum)` is statically lowered to enum values, but those values
/// still obey the same source-position fence as a direct type/member read.
/// Compiler metadata must not make the pending enum observable before its
/// `RegisterEnum` instruction (Issue #11635).
#[test]
fn pending_enum_instances_waits_for_registration_11635() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_pending_instances11635 = 1").success);

        let result = session.eval(
            "pending_instances_hidden11635 = try\n  \
                 instances(PendingInstancesEnum11635); false\n\
             catch e\n  e isa UndefVarError\nend; \
             @enum PendingInstancesEnum11635 \
                 pending_instances_zero11635 pending_instances_one11635; \
             pending_instances_hidden11635",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// A later same-named enum is still pending after the first registration, but
/// it must not re-hide the enum generation that was just published. Member
/// construction for both declarations completes in source order (Issue #11635).
#[test]
fn later_same_named_enum_does_not_hide_published_generation_11635() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_same_enum_name11635 = 1").success);

        let declarations = session.eval(
            "@enum SameNameEnum11635 same_name_first11635; \
             same_name_first_type_visible11635 = \
                 SameNameEnum11635 === typeof(same_name_first11635); \
             @enum SameNameEnum11635 same_name_second11635; \
             same_name_first_type_visible11635 && \
                 isdefined(Main, :same_name_first11635) && \
                 isdefined(Main, :same_name_second11635)",
        );
        assert!(declarations.success, "{:?}", declarations.error);
        assert!(matches!(declarations.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// A prior function compiled while a binding was absent contains a deliberate
/// undefined-binding trap. Introducing any nominal binding family must refresh
/// that older code instead of leaving the trap baked into the retained prefix
/// (Issue #11651).
#[test]
fn nominal_append_refreshes_prior_unresolved_callers_11651() {
    run_with_large_stack(|| {
        let cases = [
            (
                "forward_abstract11651() = FutureAbstract11651",
                "abstract type FutureAbstract11651 end",
                "forward_abstract11651() === FutureAbstract11651",
            ),
            (
                "forward_primitive11651() = FuturePrimitive11651",
                "primitive type FuturePrimitive11651 8 end",
                "forward_primitive11651() === FuturePrimitive11651",
            ),
            (
                "forward_enum_type11651() = FutureEnum11651",
                "@enum FutureEnum11651 future_enum_type_member11651",
                "forward_enum_type11651() === FutureEnum11651",
            ),
            (
                "forward_enum_member11651() = future_enum_member11651",
                "@enum FutureMemberEnum11651 future_enum_member11651",
                "forward_enum_member11651() === future_enum_member11651",
            ),
            // A call to an absent constructor compiles to ThrowUndefVarError.
            (
                "forward_constructor11651() = FutureConstructor11651()",
                "struct FutureConstructor11651 end",
                "forward_constructor11651() isa FutureConstructor11651",
            ),
            // An explicit `global` read compiles to LoadGlobalAny.
            (
                "function forward_global11651()\n  global FutureGlobal11651\n  FutureGlobal11651\nend",
                "abstract type FutureGlobal11651 end",
                "forward_global11651() === FutureGlobal11651",
            ),
        ];

        for (caller, definition, assertion) in cases {
            let mut session = new_session();
            let callers = session.eval(caller);
            assert!(callers.success, "caller `{caller}`: {:?}", callers.error);

            let definitions = session.eval(definition);
            assert!(
                definitions.success,
                "definition `{definition}`: {:?}",
                definitions.error
            );
            assert_ne!(
                session.last_vm_build_nanos(),
                Some(0),
                "`{definition}` must conservatively refresh `{caller}`"
            );

            let refreshed = session.eval(assertion);
            assert!(
                refreshed.success,
                "assertion `{assertion}`: {:?}",
                refreshed.error
            );
            assert!(matches!(refreshed.value, Some(Value::Bool(true))));
        }
    });
}

/// A child cannot publish while its source-later parent is still pending. The
/// failing marker contributes no reached activation, for every nominal child
/// family (Issue #11635).
#[test]
fn pending_parent_rejects_nominal_child_activation_11635() {
    run_with_large_stack(|| {
        let mut abstract_session = new_session();
        assert!(
            abstract_session
                .eval("seed_pending_abstract11635 = 1")
                .success
        );
        let abstract_result = abstract_session.eval(
            "abstract type PendingAbstractChild11635 <: \
                 PendingAbstractParent11635 end; \
             abstract type PendingAbstractParent11635 end",
        );
        assert!(!abstract_result.success);
        let abstract_absent = abstract_session.eval(
            "!isdefined(Main, :PendingAbstractChild11635) && \
             !isdefined(Main, :PendingAbstractParent11635)",
        );
        assert!(abstract_absent.success, "{:?}", abstract_absent.error);
        assert!(matches!(abstract_absent.value, Some(Value::Bool(true))));

        let mut concrete_session = new_session();
        assert!(
            concrete_session
                .eval("seed_pending_concrete11635 = 1")
                .success
        );
        let concrete_result = concrete_session.eval(
            "struct PendingConcreteChild11635 <: PendingConcreteParent11635 end; \
             abstract type PendingConcreteParent11635 end",
        );
        assert!(!concrete_result.success);
        let concrete_absent = concrete_session.eval(
            "!isdefined(Main, :PendingConcreteChild11635) && \
             !isdefined(Main, :PendingConcreteParent11635)",
        );
        assert!(concrete_absent.success, "{:?}", concrete_absent.error);
        assert!(matches!(concrete_absent.value, Some(Value::Bool(true))));

        let mut primitive_session = new_session();
        assert!(
            primitive_session
                .eval("seed_pending_primitive11635 = 1")
                .success
        );
        let primitive_result = primitive_session.eval(
            "primitive type PendingPrimitiveChild11635 <: \
                 PendingPrimitiveParent11635 8 end; \
             abstract type PendingPrimitiveParent11635 end",
        );
        assert!(!primitive_result.success);
        let primitive_absent = primitive_session.eval(
            "!isdefined(Main, :PendingPrimitiveChild11635) && \
             !isdefined(Main, :PendingPrimitiveParent11635)",
        );
        assert!(primitive_absent.success, "{:?}", primitive_absent.error);
        assert!(matches!(primitive_absent.value, Some(Value::Bool(true))));
    });
}

/// Upstream publishes the enum type before rejecting a member constant that
/// collides with an existing global. Exact-prefix recovery keeps that type but
/// must never overwrite the old global (Issue #11652).
#[test]
fn enum_member_collision_preserves_existing_global_11652() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("existing_enum_member11652 = 7").success);

        let declaration = session.eval(
            "@enum CollisionEnum11652 existing_enum_member11652 \
                 fresh_enum_member11652",
        );
        assert!(!declaration.success);
        assert!(session.has_live_vm());

        let preserved = session.eval(
            "fresh_enum_member_hidden11652 = try\n  \
                 fresh_enum_member11652; false\n\
             catch e\n  e isa UndefVarError\nend; \
             isdefined(Main, :CollisionEnum11652) && \
             existing_enum_member11652 == 7 && \
             existing_enum_member11652 isa Int && \
             !isdefined(Main, :fresh_enum_member11652) && \
             fresh_enum_member_hidden11652",
        );
        assert!(preserved.success, "{:?}", preserved.error);
        assert!(matches!(preserved.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let forced_rebuild = session.eval("module ForceEnumReplay11652 end");
        assert!(forced_rebuild.success, "{:?}", forced_rebuild.error);
        let still_preserved = session.eval(
            "isdefined(Main, :CollisionEnum11652) && \
             existing_enum_member11652 == 7 && \
             !isdefined(Main, :fresh_enum_member11652)",
        );
        assert!(still_preserved.success, "{:?}", still_preserved.error);
        assert!(matches!(still_preserved.value, Some(Value::Bool(true))));
    });
}

/// Upstream `@enum` keeps member metadata in source order, but emits the
/// constant declarations by iterating its integer-keyed `namemap` Dict. For
/// values 0, 1, 2 that order is 0, 2, 1, so a source-later member is already
/// published when the middle declaration collides. Recovery must retain that
/// observed prefix across a later full rebuild (Issue #11656).
#[test]
fn enum_member_collision_follows_upstream_dict_expansion_order_11656() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("existing_enum_member11656 = 7").success);

        let declaration = session.eval(
            "@enum CollisionOrderEnum11656 first_enum_member11656 \
                 existing_enum_member11656 later_enum_member11656",
        );
        assert!(!declaration.success);
        assert!(session.has_live_vm());

        let observed = session.eval(
            "isdefined(Main, :CollisionOrderEnum11656) && \
             isdefined(Main, :first_enum_member11656) && \
             existing_enum_member11656 == 7 && \
             existing_enum_member11656 isa Int && \
             isdefined(Main, :later_enum_member11656)",
        );
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let forced_rebuild = session.eval("module ForceEnumReplay11656 end");
        assert!(forced_rebuild.success, "{:?}", forced_rebuild.error);
        let replayed = session.eval(
            "isdefined(Main, :CollisionOrderEnum11656) && \
             isdefined(Main, :first_enum_member11656) && \
             existing_enum_member11656 == 7 && \
             isdefined(Main, :later_enum_member11656)",
        );
        assert!(replayed.success, "{:?}", replayed.error);
        assert!(matches!(replayed.value, Some(Value::Bool(true))));
    });
}

/// Julia's Dict insertion also rehashes when a probe chain grows too long,
/// independently of load factor. These values collide modulo the intermediate
/// table size; upstream visits source indices 0, 7, ... after the early rehash,
/// so a collision at index 7 publishes only index 0 (Issue #11656).
#[test]
fn enum_member_collision_follows_probe_limit_rehash_order_11656() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("probe_collision_member11656 = 7").success);

        let declaration = session.eval(
            "@enum ProbeOrderEnum11656 \
                 probe_member_0_11656=5 probe_member_1_11656=78 \
                 probe_member_2_11656=169 probe_member_3_11656=176 \
                 probe_member_4_11656=260 probe_member_5_11656=316 \
                 probe_member_6_11656=352 probe_collision_member11656=402 \
                 probe_member_8_11656=456 probe_member_9_11656=551 \
                 probe_member_10_11656=729 probe_member_11_11656=799 \
                 probe_member_12_11656=924 probe_member_13_11656=971 \
                 probe_member_14_11656=1015 probe_member_15_11656=1102 \
                 probe_member_16_11656=1193",
        );
        assert!(!declaration.success);
        assert!(session.has_live_vm());

        let observed = session.eval(
            "isdefined(Main, :ProbeOrderEnum11656) && \
             isdefined(Main, :probe_member_0_11656) && \
             probe_collision_member11656 == 7 && \
             !isdefined(Main, :probe_member_1_11656) && \
             !isdefined(Main, :probe_member_3_11656) && \
             !isdefined(Main, :probe_member_16_11656)",
        );
        assert!(observed.success, "{:?}", observed.error);
        assert!(matches!(observed.value, Some(Value::Bool(true))));

        let forced_rebuild = session.eval("module ForceProbeEnumReplay11656 end");
        assert!(forced_rebuild.success, "{:?}", forced_rebuild.error);
        let replayed = session.eval(
            "isdefined(Main, :probe_member_0_11656) && \
             probe_collision_member11656 == 7 && \
             !isdefined(Main, :probe_member_1_11656) && \
             !isdefined(Main, :probe_member_16_11656)",
        );
        assert!(replayed.success, "{:?}", replayed.error);
        assert!(matches!(replayed.value, Some(Value::Bool(true))));
    });
}

/// Duplicate values and names are macro-expansion errors upstream, so neither
/// the enum type nor any member constant may be published (Issue #11666).
#[test]
fn enum_duplicate_members_are_rejected_before_publication_11666() {
    run_with_large_stack(|| {
        let cases = [
            (
                "@enum DuplicateValueEnum11666 duplicate_value_a11666=1 \
                     duplicate_value_b11666=1",
                "!isdefined(Main, :DuplicateValueEnum11666) && \
                 !isdefined(Main, :duplicate_value_a11666) && \
                 !isdefined(Main, :duplicate_value_b11666)",
            ),
            (
                "@enum DuplicateNameEnum11666 duplicate_name11666=1 \
                     duplicate_name11666=2",
                "!isdefined(Main, :DuplicateNameEnum11666) && \
                 !isdefined(Main, :duplicate_name11666)",
            ),
        ];

        for (declaration, assertion) in cases {
            let mut session = new_session();
            assert!(session.eval("duplicate_seed11666 = 1").success);
            let rejected = session.eval(declaration);
            assert!(!rejected.success, "`{declaration}` unexpectedly succeeded");

            let unpublished = session.eval(assertion);
            assert!(unpublished.success, "{:?}", unpublished.error);
            assert!(matches!(unpublished.value, Some(Value::Bool(true))));
        }
    });
}

/// A live nominal declaration is not only source-visible in its defining
/// input: its identity, hierarchy, methods, reflection, enum members, and enum
/// display registry must all remain usable by later delta compiles without
/// replacing the retained VM (Issue #9784).
#[test]
fn nominal_types_persist_and_dispatch_without_vm_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_nominal_persistence9784 = 1").success);

        let definitions = session.eval(
            "abstract type LiveAnimal9784 end; \
             struct LiveDog9784 <: LiveAnimal9784\n  x::Int\nend; \
             live_speak9784(x::LiveAnimal9784) = x.x + 1; \
             primitive type LiveByte9784 <: Unsigned 8 end; \
             live_primitive_bits9784(::Type{LiveByte9784}) = sizeof(LiveByte9784); \
             @enum LiveColor9784 live_red9784 live_blue9784",
        );
        assert!(definitions.success, "{:?}", definitions.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let later_read = session.eval(
            "live_speak9784(LiveDog9784(41)) == 42 && \
             live_primitive_bits9784(LiveByte9784) == 1 && \
             LiveByte9784 <: Unsigned && \
             LiveColor9784(1) === live_blue9784 && \
             instances(LiveColor9784) === (live_red9784, live_blue9784) && \
             string(live_blue9784) == \"live_blue9784\"",
        );
        assert!(later_read.success, "{:?}", later_read.error);
        assert!(matches!(later_read.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

#[test]
fn pending_struct_binding_is_hidden_inside_called_function_11546() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_type_body_order11546 = 1").success);

        let result = session.eval(
            "future_type_value11546() = FutureBodyType11546; \
             future_type_ctor11546() = FutureBodyType11546(1); \
             value_hidden11546 = try\n  future_type_value11546(); false\ncatch e\n  e isa UndefVarError\nend; \
             ctor_hidden11546 = try\n  future_type_ctor11546(); false\ncatch e\n  e isa UndefVarError\nend; \
             struct FutureBodyType11546\n  x::Int\nend; \
             value_hidden11546 && ctor_hidden11546 && future_type_ctor11546().x == 1",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// A concrete type whose declaration ran before a later exception is part of
/// the committed REPL definition transaction. A declaration after the throw is
/// not, and the next appended type must reuse the discarded suffix's aligned ID
/// without rebuilding the VM (Issues #9784/#11546).
#[test]
fn reached_struct_prefix_survives_later_error_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_type_prefix9784 = 1").success);

        let failed = session.eval(
            "struct ReachedType9784\n  x::Int\nend; \
             reached_value9784 = ReachedType9784(41); \
             error(\"type prefix boom\"); \
             struct UnreachedType9784\n  y::Int\nend",
        );
        assert!(!failed.success);
        assert!(session.has_live_vm());

        let live = session.eval(
            "ReachedType9784(reached_value9784.x + 1).x == 42 && \
             !isdefined(Main, :UnreachedType9784)",
        );
        assert!(live.success, "{:?}", live.error);
        assert!(matches!(live.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let replacement =
            session.eval("struct ReplacementType9784\n  z::Int\nend; ReplacementType9784(42).z");
        assert!(replacement.success, "{:?}", replacement.error);
        assert!(matches!(replacement.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let fallback = session.eval(
            "let q = ReachedType9784(42); q.x == 42 && !isdefined(Main, :UnreachedType9784) end",
        );
        assert!(fallback.success, "{:?}", fallback.error);
        assert!(matches!(fallback.value, Some(Value::Bool(true))));
    });
}

/// If the exception precedes every type marker, recovery commits an empty type
/// delta and frees the reserved IDs for a later real declaration (Issue #9784).
#[test]
fn zero_reached_struct_suffix_is_discarded_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_zero_type9784 = 1").success);
        assert!(
            !session
                .eval("error(\"before type\"); struct ReusableType9784; x::Int; end")
                .success
        );
        assert!(session.has_live_vm());

        let retried =
            session.eval("struct ReusableType9784\n  x::Int\nend; ReusableType9784(42).x");
        assert!(retried.success, "{:?}", retried.error);
        assert!(matches!(retried.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

include!("internal/callable_singleton_identity_11685_tests.rs");

/// Function and type declarations share one source chronology. Independent
/// per-kind counts could accept a reordered or skipped marker; this row pins the
/// exact interleaved prefix that executed before the exception (Issue #9784).
#[test]
fn interleaved_function_and_struct_prefix_is_exact_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_mixed_prefix9784 = 1").success);
        assert!(
            !session
                .eval(
                    "before_mixed9784() = 40; \
                 struct MiddleMixed9784\n  x::Int\nend; \
                 error(\"mixed prefix\"); \
                 struct AfterMixed9784\n  x::Int\nend; after_mixed9784() = 99",
                )
                .success
        );
        assert!(session.has_live_vm());

        let result = session.eval(
            "before_mixed9784() + MiddleMixed9784(2).x == 42 && \
             !isdefined(Main, :AfterMixed9784) && !isdefined(Main, :after_mixed9784)",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// Every nominal declaration family participates in the same source-order
/// transaction. After a catchable error, the retained VM and persistent
/// compiler snapshot must agree on the exact typed prefix that actually ran;
/// declarations and enum members after the throw remain absent, and their
/// aligned suffix slots can be reused by the next live append (Issue #9784).
#[test]
fn interleaved_nominal_definition_prefix_recovers_exactly_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("seed_nominal_prefix9784 = 1").success);

        let failed = session.eval(
            "reached_nominal_fn9784() = 40; \
             abstract type ReachedAbstractNominal9784 end; \
             struct ReachedConcreteNominal9784 <: ReachedAbstractNominal9784\n  x::Int\nend; \
             primitive type ReachedPrimitiveNominal9784 <: Unsigned 8 end; \
             @enum ReachedEnumNominal9784 reached_enum_zero9784 reached_enum_one9784; \
             error(\"nominal prefix boom\"); \
             abstract type UnreachedAbstractNominal9784 end; \
             primitive type UnreachedPrimitiveNominal9784 8 end; \
             @enum UnreachedEnumNominal9784 unreached_enum_zero9784 unreached_enum_one9784",
        );
        assert!(!failed.success, "the catchable error must be reported");
        assert!(
            session.has_live_vm(),
            "the exact reached nominal prefix must retain the live VM"
        );

        let reached = session.eval(
            "reached_nominal_fn9784() == 40 && \
             ReachedConcreteNominal9784 <: ReachedAbstractNominal9784 && \
             ReachedConcreteNominal9784(42).x == 42 && \
             sizeof(ReachedPrimitiveNominal9784) == 1 && \
             ReachedPrimitiveNominal9784 <: Unsigned && \
             ReachedEnumNominal9784(1) === reached_enum_one9784 && \
             instances(ReachedEnumNominal9784) === \
                 (reached_enum_zero9784, reached_enum_one9784) && \
             string(reached_enum_one9784) == \"reached_enum_one9784\" && \
             !isdefined(Main, :UnreachedAbstractNominal9784) && \
             !isdefined(Main, :UnreachedPrimitiveNominal9784) && \
             !isdefined(Main, :UnreachedEnumNominal9784) && \
             !isdefined(Main, :unreached_enum_zero9784) && \
             !isdefined(Main, :unreached_enum_one9784)",
        );
        assert!(reached.success, "{:?}", reached.error);
        assert!(matches!(reached.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let replacement = session.eval(
            "abstract type ReplacementAbstractNominal9784 end; \
             primitive type ReplacementPrimitiveNominal9784 16 end; \
             @enum ReplacementEnumNominal9784 replacement_enum_zero9784 \
                 replacement_enum_one9784; \
             isdefined(Main, :ReplacementAbstractNominal9784) && \
             sizeof(ReplacementPrimitiveNominal9784) == 2 && \
             instances(ReplacementEnumNominal9784) === \
                 (replacement_enum_zero9784, replacement_enum_one9784)",
        );
        assert!(replacement.success, "{:?}", replacement.error);
        assert!(matches!(replacement.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

#[test]
fn reached_function_definition_survives_later_error_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let old = session.eval("old_snapshot9784() = 40");
        assert!(old.success, "{:?}", old.error);

        let failed = session.eval("new_snapshot9784() = 41; error(\"definition boom\")");
        assert!(!failed.success, "the appended main must report its error");
        assert!(
            session.has_live_vm(),
            "a reached definition and catchable error must retain the aligned live world"
        );

        let resumed = session.eval("new_snapshot9784() + 1");
        assert!(resumed.success, "{:?}", resumed.error);
        assert!(matches!(resumed.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Marker-less HOF helpers and marker-gated source methods may share one
/// append. The helper is immediately callable, while the named method becomes
/// visible at its source marker and persists into the next eval (Issue #9784).
#[test]
fn hof_helper_and_named_method_share_live_append_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("hof_helper_warm_9784 = 1").success);

        let mixed = session.eval(
            "hof_named_9784(x) = x + 10; hof_named_result_9784 = ntuple(i -> hof_named_9784(i), 2)",
        );
        assert!(mixed.success, "{:?}", mixed.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let tuple = session.eval("hof_named_result_9784");
        assert!(matches!(
            tuple.value.as_ref(),
            Some(Value::Tuple(value))
                if matches!(value.elements.as_slice(), [Value::I64(11), Value::I64(12)])
        ));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let persisted = session.eval("hof_named_9784(5)");
        assert!(persisted.success, "{:?}", persisted.error);
        assert!(matches!(persisted.value, Some(Value::I64(15))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Error recovery counts only source-marker activations. Marker-less helpers
/// before either a reached or unreached source method stay installed without
/// making that method visible early (Issue #9784).
#[test]
fn hof_helpers_preserve_exact_error_definition_prefix_9784() {
    run_with_large_stack(|| {
        let mut reached_session = new_session();
        assert!(reached_session.eval("hof_reached_warm_9784 = 1").success);
        let reached_error = reached_session.eval(
            "hof_saved_9784 = i -> i + 2; ntuple(i -> i, 2); hof_reached_9784(x) = x + 1; error(\"stop\")",
        );
        assert!(!reached_error.success);
        assert!(reached_session.has_live_vm());
        assert_eq!(reached_session.last_vm_build_nanos(), Some(0));
        let helper = reached_session.eval("hof_saved_9784(3)");
        assert!(helper.success, "{:?}", helper.error);
        assert!(matches!(helper.value, Some(Value::I64(5))));
        assert_eq!(reached_session.last_vm_build_nanos(), Some(0));
        let reached = reached_session.eval("hof_reached_9784(4)");
        assert!(reached.success, "{:?}", reached.error);
        assert!(matches!(reached.value, Some(Value::I64(5))));

        let mut unreached_session = new_session();
        assert!(
            unreached_session
                .eval("hof_unreached_warm_9784 = 1")
                .success
        );
        let unreached_error = unreached_session
            .eval("ntuple(i -> i, 2); error(\"stop\"); hof_unreached_9784(x) = x + 1");
        assert!(!unreached_error.success);
        assert!(unreached_session.has_live_vm());
        assert_eq!(unreached_session.last_vm_build_nanos(), Some(0));
        let unreached = unreached_session.eval("isdefined(Main, :hof_unreached_9784)");
        assert!(unreached.success, "{:?}", unreached.error);
        assert!(matches!(unreached.value, Some(Value::Bool(false))));
    });
}

/// Error projection must update the accumulated source-definition mirror with
/// the reached source primary, not the first raw function-table entry (which may
/// be a marker-less helper). A later macro-bearing input forces a full rebuild
/// and must still see the reached method (Issue #9784).
#[test]
fn hof_reached_method_survives_error_then_full_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("hof_rebuild_warm_9784 = 1").success);

        let failed = session
            .eval("map(x -> x + 1, [1]); hof_rebuild_reached_9784(x) = x + 1; error(\"stop\")");
        assert!(!failed.success);
        assert!(session.has_live_vm());
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let rebuilt =
            session.eval("macro hof_force_rebuild_9784(); :(1); end; hof_rebuild_reached_9784(41)");
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(
            matches!(rebuilt.value, Some(Value::I64(42))),
            "unexpected rebuilt value: {:?}",
            rebuilt.value
        );
        assert_ne!(
            session.last_vm_build_nanos(),
            Some(0),
            "a macro-bearing input must exercise the accumulated full rebuild"
        );
    });
}

/// Generated-looking spelling is not helper provenance for block-local source
/// methods either. A lexical method may execute inside its scope, but it must
/// not be persisted by the helper collector or revived as a Main method by a
/// later full rebuild (Issue #9784).
#[test]
fn helper_like_unreached_block_method_stays_hidden_after_full_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("hof_block_name_warm_9784 = 1").success);

        let scoped = session.eval(
            "let; function __lambda_user_block_9784(x); x + 1; end; __lambda_user_block_9784(1); end",
        );
        assert!(scoped.success, "{:?}", scoped.error);
        assert!(matches!(scoped.value, Some(Value::I64(2))));

        let rebuilt = session.eval(
            "macro hof_block_force_rebuild_9784(); :(1); end; isdefined(Main, :__lambda_user_block_9784)",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::Bool(false))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// On a full fallback, `repl_current_function_count` counts source methods, not
/// marker-less helpers. Otherwise a helper-only current input consumes the
/// budget and incorrectly makes the first merged prior method dormant.
#[test]
fn prior_method_survives_helper_bearing_full_fallback_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let prior = session.eval("prior_helper_full_9784(x) = x + 1");
        assert!(prior.success, "{:?}", prior.error);

        let rebuilt = session.eval(
            "macro helper_full_force_9784(); :(1); end; sum(prior_helper_full_9784(x) for x in 41:41) == 42",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::Bool(true))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Same-signature definitions in one input replace the final compiler method
/// row, but error recovery must reconstruct the row for the reached definition
/// rather than merely delete the dormant final row (Issue #9784).
#[test]
fn reached_same_signature_redefinition_survives_error_and_full_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let initial = session.eval("same_sig_prefix_9784(x) = x + 1");
        assert!(initial.success, "{:?}", initial.error);

        let failed = session.eval(
            "same_sig_prefix_9784(x) = x + 2; error(\"stop\"); same_sig_prefix_9784(x) = x + 3",
        );
        assert!(!failed.success);
        assert!(session.has_live_vm());
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let live = session.eval("same_sig_prefix_9784(40)");
        assert!(live.success, "{:?}", live.error);
        assert!(matches!(live.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let rebuilt = session
            .eval("macro same_sig_force_rebuild_9784(); :(1); end; same_sig_prefix_9784(40)");
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::I64(42))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Partial recovery must discard inference results computed for an unreached
/// same-signature replacement. The cache key identifies the method signature,
/// not which source body supplied its return type (Issue #9784).
#[test]
fn unreached_same_signature_body_does_not_poison_inference_cache_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("same_sig_cache_warm_9784 = 1").success);
        let failed = session.eval(
            "same_sig_cache_9784(x) = x + 2; error(\"stop\"); same_sig_cache_9784(x) = \"bad\"",
        );
        assert!(!failed.success);
        assert!(session.has_live_vm());

        let result = session.eval("same_sig_cache_9784(40) + 1");
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(43))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Dormant methods compiled after a throwing statement are not in the current
/// world. Reflection must use the same visibility gate as runtime dispatch.
#[test]
fn reflection_hides_unreached_dormant_method_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("reflection_world_warm_9784 = 1").success);
        assert!(
            session
                .eval("reflection_world_9784(x::Int64) = x + 1")
                .success
        );

        let failed = session.eval("error(\"stop\"); reflection_world_9784(x::Float64) = x + 3.0");
        assert!(!failed.success);
        assert!(session.has_live_vm());

        let reflected =
            session.eval("isempty(Base.return_types(reflection_world_9784, Tuple{Float64}))");
        assert!(reflected.success, "{:?}", reflected.error);
        assert!(matches!(reflected.value, Some(Value::Bool(true))));

        let call = session.eval("reflection_world_9784(1.0)");
        assert!(!call.success, "dormant Float64 method became callable");
    });
}

/// Reflection on an explicitly carried callable must follow that callable's
/// frozen helper provenance instead of rediscovering only Julia-visible source
/// methods by spelling. The private helper is hidden from bare-name reflection,
/// but remains reflectable through the value that denotes it (Issue #9784).
#[test]
fn reflection_accepts_explicit_lowering_helper_callable_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let saved = session.eval("reflected_helper_9784 = identity(x -> x + 1)");
        assert!(saved.success, "{:?}", saved.error);

        for source in [
            "Base.return_types(reflected_helper_9784, Tuple{Int64}) == Any[Int64]",
            "Base.infer_return_type(reflected_helper_9784, Tuple{Int64}) == Int64",
            "hasmethod(reflected_helper_9784, Tuple{Int64})",
            "which(reflected_helper_9784, Tuple{Int64}) isa Method",
        ] {
            let reflected = session.eval(source);
            assert!(reflected.success, "{source}: {:?}", reflected.error);
            assert!(
                matches!(reflected.value, Some(Value::Bool(true))),
                "{source}: {:?}",
                reflected.value
            );
        }
    });
}

/// Julia method replacement is last-definition-wins for an equal signature.
/// Reflection must deduplicate the stale row just like runtime dispatch rather
/// than reporting an artificial ambiguity (Issue #9784).
#[test]
fn reflection_same_signature_redefinition_is_last_wins_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("reflected_redefined_9784(x) = 1").success);
        assert!(session.eval("reflected_redefined_9784(x) = 2").success);

        for source in [
            "reflected_redefined_9784(1) == 2",
            "hasmethod(reflected_redefined_9784, Tuple{Int64})",
            "which(reflected_redefined_9784, Tuple{Int64}) isa Method",
        ] {
            let reflected = session.eval(source);
            assert!(reflected.success, "{source}: {:?}", reflected.error);
            assert!(
                matches!(reflected.value, Some(Value::Bool(true))),
                "{source}: {:?}",
                reflected.value
            );
        }
    });
}

/// MethodTable canonicalizes a covariant type variable used once to its upper
/// bound, so these two source spellings are one Julia method identity. Runtime
/// reflection must apply the same canonical last-definition-wins rule rather
/// than retain two raw-projection rows and report ambiguity (Issue #9784).
#[test]
fn reflection_canonical_equivalent_signature_is_last_wins_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval("canonical_reflection_9784(x::T) where {T<:Number} = 1")
                .success
        );
        assert!(
            session
                .eval("canonical_reflection_9784(x::Number) = 2")
                .success
        );

        for source in [
            "canonical_reflection_9784(1) == 2",
            "length(methods(canonical_reflection_9784)) == 1",
            "hasmethod(canonical_reflection_9784, Tuple{Int64})",
            "which(canonical_reflection_9784, Tuple{Int64}) isa Method",
        ] {
            let reflected = session.eval(source);
            assert!(reflected.success, "{source}: {:?}", reflected.error);
            assert!(
                matches!(reflected.value, Some(Value::Bool(true))),
                "{source}: {:?}",
                reflected.value
            );
        }
    });
}

/// Composition stores callable values, not public generic names. Both the
/// innermost and every pending outer must dispatch through the values' frozen
/// helper provenance after private helpers leave the public name index
/// (Issue #9784).
#[test]
fn composed_lambdas_use_carried_helper_provenance_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let defined = session.eval("composed_helpers_9784 = (x -> x + 1) ∘ (x -> 2x)");
        assert!(defined.success, "{:?}", defined.error);

        let result = session.eval("composed_helpers_9784(20)");
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(41))));
    });
}

/// `invoke` on a captured lambda must resolve the closure through the private
/// helper index, not through the Julia-visible public method index from which
/// lowering helpers are intentionally absent (Issue #9784).
#[test]
fn invoke_captured_lambda_uses_helper_provenance_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let defined = session.eval("captured_invoke_9784 = let a = 10; x -> x + a; end");
        assert!(defined.success, "{:?}", defined.error);

        let result = session.eval("invoke(captured_invoke_9784, Tuple{Int64}, 1)");
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(11))));
    });
}

/// A reshaped array view reads elements from its shared parent. Callable
/// rebasing on full rebuild must therefore traverse that parent, not only the
/// view's local data buffer (Issue #9784). The separate `Expr(...)` fragment
/// namespace gap discovered while isolating this path is Issue #11676.
#[test]
fn reshaped_array_parent_rebases_carried_callable_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval("shared_parent_callable_9784 = reshape(Any[x -> x + 2], 1, 1)")
                .success
        );
        let before = session.eval("shared_parent_callable_9784[1](40)");
        assert!(before.success, "{:?}", before.error);
        assert!(matches!(before.value, Some(Value::I64(42))));

        // A macro definition forces the full path, while the new constructor
        // shifts indices and the explicit rebinding forces exact Value seeding
        // instead of source reconstruction. Rebasing must therefore operate on
        // the reshaped carrier's shared parent.
        let rebuilt = session.eval(
            "macro force_shared_parent_rebuild_9784(); :(1); end; struct ForceExprArgs9784; x::Int; end; shared_parent_callable_9784 = shared_parent_callable_9784; shared_parent_callable_9784[1](40)",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::I64(42))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// A lambda lifted from a top-level call argument lives in `Program.functions`,
/// not in the main-inline collector. Recovery counts source activations rather
/// than raw helper positions, so the reached closure body must remain available
/// after an accumulated full rebuild (Issue #9784).
#[test]
fn reached_call_argument_lambda_survives_error_and_full_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("call_arg_lambda_warm_9784 = 1").success);

        let failed = session.eval("saved_call_arg_9784 = identity(i -> i + 2); error(\"stop\")");
        assert!(!failed.success);
        assert!(session.has_live_vm());
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let rebuilt =
            session.eval("macro call_arg_force_rebuild_9784(); :(1); end; saved_call_arg_9784(40)");
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::I64(42))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// An unprojectable full-path error drops the live VM but retains globals from
/// the last successful transaction. Frozen callable indices must still rebase
/// from the retained identity snapshot on the next full rebuild (Issue #9784).
#[test]
fn carried_lambda_survives_dropped_vm_then_full_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let saved = session.eval("saved_after_vm_drop_9784 = identity(i -> i + 2)");
        assert!(saved.success, "{:?}", saved.error);

        let dropped =
            session.eval("macro drop_live_vm_9784(); :(1); end; error(\"drop the fresh VM\")");
        assert!(!dropped.success);
        assert!(!session.has_live_vm());

        let rebuilt = session.eval(
            "macro rebuild_after_drop_9784(); :(1); end; shifted_helper_9784 = identity(j -> j); saved_after_vm_drop_9784(40)",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::I64(42))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Mutable Rc-backed roots must be detached before candidate rebasing even when
/// no struct heap exists. A failed rebuild must not rewrite the session-owned
/// callable twice on the following retry.
#[test]
fn callable_nested_in_any_array_survives_failed_then_successful_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let saved = session.eval("nested_callable_array_9784 = Any[identity(x -> x + 2)]");
        assert!(saved.success, "{:?}", saved.error);

        let failed = session.eval(
            "macro fail_nested_array_rebuild_9784(); :(1); end; shifted_nested_array_9784 = identity(y -> y); error(\"drop\")",
        );
        assert!(!failed.success);
        assert!(!session.has_live_vm());

        let rebuilt = session.eval(
            "macro retry_nested_array_rebuild_9784(); :(1); end; another_shifted_nested_array_9784 = identity(z -> z); nested_callable_array_9784[1](40)",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::I64(42))));
    });
}

/// Mutating an existing container does not execute StoreGlobal, but it can make
/// a freshly appended helper observable. Error recovery must retain that helper
/// and must not classify the append as an unreachable private tail (#9784).
#[test]
fn helper_stored_in_existing_container_survives_error_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval("observed_helper_box_9784 = Any[nothing]")
                .success
        );

        let failed =
            session.eval("observed_helper_box_9784[1] = identity(x -> x + 1); error(\"stop\")");
        assert!(!failed.success);
        assert!(session.has_live_vm());

        let call = session.eval("observed_helper_box_9784[1](41)");
        assert!(call.success, "{:?}", call.error);
        assert!(matches!(call.value, Some(Value::I64(42))));
    });
}

/// Discarding an unreachable helper tail must not discard the recovered VM.
/// Otherwise the next full-path error loses mutations completed before its
/// exception because no live checkpoint remains to publish them (#9784).
#[test]
fn helper_tail_rollback_preserves_later_failed_mutation_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("rollback_state_9784 = [0]").success);

        let helper_failure = session.eval("error(\"stop\"); identity(x -> x + 1)");
        assert!(!helper_failure.success);
        assert!(
            session.has_live_vm(),
            "unreachable helper code should be rolled back without dropping runtime state"
        );

        let mutation_failure = session.eval("rollback_state_9784[1] = 7; error(\"stop\")");
        assert!(!mutation_failure.success);
        assert!(
            session.has_live_vm(),
            "mutation recovery dropped VM: error={:?}, build={:?}",
            mutation_failure.error,
            session.last_vm_build_nanos()
        );

        let retained = session.eval("rollback_state_9784[1]");
        assert!(retained.success, "{:?}", retained.error);
        assert!(matches!(retained.value, Some(Value::I64(7))));
    });
}

/// Static generator callables carry raw function indices too. Rebasing must
/// reach a generator nested inside a transplanted struct field.
#[test]
fn generator_callable_nested_in_struct_survives_full_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("struct GeneratorBox9784; g; end").success);
        assert!(session.eval("generator_map_9784(x) = x * 10").success);
        let saved = session
            .eval("boxed_generator_9784 = GeneratorBox9784((generator_map_9784(x) for x in 1:2))");
        assert!(saved.success, "{:?}", saved.error);

        let rebuilt = session.eval(
            "macro force_boxed_generator_rebuild_9784(); :(1); end; shift_boxed_generator_9784 = identity(q -> q); collect(boxed_generator_9784.g) == [10, 20]",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(rebuilt.value, Some(Value::Bool(true))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// A saved generic function is a family hint, not an immutable body pointer.
/// Multiple same-signature rows in a full rebuild are legitimate source-order
/// redefinitions and runtime dispatch must retain last-definition-wins.
#[test]
fn saved_generic_rebases_across_same_signature_redefinitions_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("redefined_saved_9784() = 1").success);
        assert!(
            session
                .eval("saved_generic_9784 = redefined_saved_9784")
                .success
        );

        let redefined = session.eval(
            "macro force_saved_generic_rebuild_9784(); :(1); end; redefined_saved_9784() = 10; redefined_saved_9784() = 20",
        );
        assert!(redefined.success, "{:?}", redefined.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));

        let call = session.eval("saved_generic_9784()");
        assert!(call.success, "{:?}", call.error);
        assert!(matches!(call.value, Some(Value::I64(20))));
    });
}

/// Recovery must not apply syntactically present value rebindings that occur
/// after the throw. In particular, an unreachable value assignment cannot
/// remove a previously persisted static type alias before a later full rebuild
/// lowers a signature through it (Issue #9784).
#[test]
fn unreachable_alias_rebinding_does_not_poison_full_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("RecoveryAlias9784 = Int").success);

        let failed =
            session.eval("alias_reached_9784() = 1; error(\"stop\"); RecoveryAlias9784 = 42");
        assert!(!failed.success);
        assert!(session.has_live_vm());
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let rebuilt = session.eval(
            "macro alias_force_rebuild_9784(); :(1); end; alias_after_recovery_9784(x::RecoveryAlias9784) = x + 1; alias_result_9784 = alias_after_recovery_9784(41)",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));
        let result = session.eval("alias_result_9784");
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::I64(42))));
    });
}

/// A global write executed inside a called function is authoritative even when
/// the enclosing REPL input later throws. Recovery must invalidate a persisted
/// static alias with that name before the next full rebuild (#9784).
#[test]
fn called_function_alias_rebinding_invalidates_recovered_metadata_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("CalledAlias9784 = Int64").success);
        assert!(
            session
                .eval("mutate_called_alias_9784() = (global CalledAlias9784 = 7)")
                .success
        );

        let failed = session.eval("mutate_called_alias_9784(); error(\"stop\")");
        assert!(!failed.success);
        let forced = session.eval("macro force_called_alias_rebuild_9784(); :(1); end");
        assert!(forced.success, "{:?}", forced.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));

        let definition = session.eval("called_alias_method_9784(x::CalledAlias9784) = 1");
        if definition.success {
            // sjulia currently accepts a value-bound annotation as an unresolved
            // nominal struct (Issue #11711). Until that validation gap is fixed,
            // dispatch is the authoritative proof that the stale Int64 alias is gone.
            let invalid_call = session.eval("called_alias_method_9784(7)");
            assert!(
                !invalid_call.success,
                "recovered value binding remained a stale type alias: {:?}",
                invalid_call.value
            );
        }
    });
}

/// Generated helper identity must be unique across REPL fragments. A later
/// generator at the same source offset cannot hijack the body of an older
/// capture-bearing lazy generator retained by the live VM (Issue #9784).
#[test]
fn generator_helpers_do_not_collide_across_live_evals_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("generator_collision_warm_9784 = 1").success);
        assert!(
            session
                .eval("let a = 10; global oldg9784 = (x + a for x in 1:2); end")
                .success
        );
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let newer = session.eval("let a = 100; global newg9784 = (x * a for x in 1:2); end");
        assert!(newer.success, "{:?}", newer.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let old = session.eval("collect(oldg9784) == [11, 12]");
        assert!(old.success, "{:?}", old.error);
        assert!(matches!(old.value, Some(Value::Bool(true))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));
    });
}

/// LambdaContext must reach every generator child: body, iterator, and filter.
/// Otherwise each fragment falls back to span-only `__lambda_nested_*` names and
/// a later eval can replace the callable retained by an older lazy generator.
/// Enclosing-`let` capture inside that nested lambda is tracked separately by
/// Issue #11672. Direct lazy-generator global persistence across a full rebuild
/// is Issue #11673, so boxes exercise the supported struct-heap transplant path.
#[test]
fn nested_generator_lambdas_keep_fragment_namespace_after_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("nested_generator_warm_9784 = 1").success);
        assert!(
            session
                .eval("struct NestedGeneratorBox9784; g; end")
                .success
        );

        for source in [
            "body_nested_g9784 = NestedGeneratorBox9784(((x -> x + 10)(i) for i in 1:2))",
            "iter_nested_g9784 = NestedGeneratorBox9784((i for i in map(x -> x + 10, [1, 2])))",
            "filter_nested_g9784 = NestedGeneratorBox9784((i for i in 1:3 if (x -> x > 10)(i)))",
            "newer_body_nested_g9784 = NestedGeneratorBox9784(((x -> x * 100)(i) for i in 1:2))",
            "newer_iter_nested_g9784 = NestedGeneratorBox9784((i for i in map(x -> x * 100, [1, 2])))",
            "newer_filter_nested_g9784 = NestedGeneratorBox9784((i for i in 1:3 if (x -> x < 100)(i)))",
        ] {
            let result = session.eval(source);
            assert!(result.success, "{source}: {:?}", result.error);
        }

        let rebuilt = session.eval(
            "macro force_nested_generator_rebuild_9784(); :(1); end; (collect(body_nested_g9784.g), collect(iter_nested_g9784.g), collect(filter_nested_g9784.g))",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));

        let check = session.eval(
            "collect(body_nested_g9784.g) == [11, 12] && collect(iter_nested_g9784.g) == [11, 12] && collect(filter_nested_g9784.g) == Int[]",
        );
        assert!(check.success, "{:?}", check.error);
        assert!(matches!(check.value, Some(Value::Bool(true))));
    });
}

/// Typed comprehensions and ranges nested inside them must carry the REPL
/// fragment's LambdaContext just like ordinary comprehensions. Two same-shaped
/// fragments must therefore retain distinct callable identities after a full
/// rebuild (Issue #9784).
#[test]
fn typed_comprehension_lambdas_keep_fragment_namespace_after_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval("old_typed_helpers_9784 = Any[(x -> x + 10) for i in ((y -> y)(1)):1]")
                .success
        );
        assert!(
            session
                .eval("new_typed_helpers_9784 = Any[(x -> x * 100) for i in ((y -> y)(1)):1]")
                .success
        );

        let rebuilt = session.eval(
            "macro force_typed_helper_rebuild_9784(); :(1); end; (old_typed_helpers_9784[1](1), new_typed_helpers_9784[1](1))",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(
            rebuilt.value,
            Some(Value::Tuple(tuple))
                if matches!(tuple.elements.as_slice(), [Value::I64(11), Value::I64(100)])
        ));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Typed matrix literals must lower every element with the enclosing REPL
/// fragment's LambdaContext. Otherwise same-shaped later fragments reuse the
/// old matrix's marker-less helper names and replace its carried callables when
/// a full rebuild accumulates both fragments (Issue #9784).
#[test]
fn typed_matrix_lambdas_keep_fragment_namespace_after_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval("old_typed_matrix_9784 = Any[(x -> x + 10) (x -> x + 20)]")
                .success
        );
        assert!(
            session
                .eval("new_typed_matrix_9784 = Any[(x -> x * 100) (x -> x * 200)]")
                .success
        );

        let rebuilt = session.eval(
            "macro force_typed_matrix_rebuild_9784(); :(1); end; (old_typed_matrix_9784[1](1), new_typed_matrix_9784[1](1))",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(
            rebuilt.value,
            Some(Value::Tuple(tuple))
                if matches!(tuple.elements.as_slice(), [Value::I64(11), Value::I64(100)])
        ));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Compiler-generated callable helpers are not Julia-visible generic methods.
/// A generated helper whose spelling collides with a prior user generic must
/// not replace or intercept the user's direct dispatch surface (Issue #9784).
#[test]
fn generated_lambda_does_not_hijack_prior_user_generic_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let user = session.eval("__lambda_repl_1_0(x) = x + 100");
        assert!(user.success, "{:?}", user.error);

        // This top-level call-argument arrow is lifted as exactly
        // `__lambda_repl_1_0`, colliding with the source method above.
        let generated = session.eval("saved_collision_9784 = identity(x -> 1.5)");
        assert!(generated.success, "{:?}", generated.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let distinct_types =
            session.eval("typeof(__lambda_repl_1_0) !== typeof(saved_collision_9784)");
        assert!(distinct_types.success, "{:?}", distinct_types.error);
        assert!(
            matches!(distinct_types.value, Some(Value::Bool(true))),
            "source and lowering-helper definition sites shared one singleton type (Issue #11685)"
        );

        let typed_dispatch = session.eval(
            "collision_type_dispatch_11685(f::typeof(__lambda_repl_1_0)) = 1; collision_type_dispatch_11685(f) = 2; (collision_type_dispatch_11685(__lambda_repl_1_0), collision_type_dispatch_11685(saved_collision_9784))",
        );
        assert!(typed_dispatch.success, "{:?}", typed_dispatch.error);
        assert!(matches!(
            typed_dispatch.value,
            Some(Value::Tuple(tuple))
                if matches!(tuple.elements.as_slice(), [Value::I64(1), Value::I64(2)])
        ));

        let closure = session.eval("saved_collision_9784(1)");
        assert!(closure.success, "{:?}", closure.error);
        assert!(matches!(closure.value, Some(Value::F64(value)) if value == 1.5));

        let direct = session.eval("__lambda_repl_1_0(1)");
        assert!(direct.success, "{:?}", direct.error);
        assert!(matches!(direct.value, Some(Value::I64(101))));

        let caller_def = session.eval("call_user_collision_9784(x) = __lambda_repl_1_0(x)");
        assert!(caller_def.success, "{:?}", caller_def.error);
        let caller_inference =
            session.eval("Base.infer_return_type(call_user_collision_9784, Tuple{Int64})");
        assert!(caller_inference.success, "{:?}", caller_inference.error);
        assert!(
            matches!(caller_inference.value, Some(Value::DataType(ref ty)) if matches!(ty.as_ref(), subset_julia_vm::types::JuliaType::Int64)),
            "a same-named private helper replaced the source method in reflection inference: {:?}",
            caller_inference.value
        );

        let helper_inference =
            session.eval("Base.infer_return_type(saved_collision_9784, Tuple{Int64})");
        assert!(helper_inference.success, "{:?}", helper_inference.error);
        assert!(
            matches!(helper_inference.value, Some(Value::DataType(ref ty)) if matches!(ty.as_ref(), subset_julia_vm::types::JuliaType::Float64)),
            "the private helper must remain independently inferable: {:?}",
            helper_inference.value
        );

        let rebuilt = session.eval(
            "macro force_collision_rebuild_9784(); :(1); end; (__lambda_repl_1_0(1), saved_collision_9784(1))",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(
            rebuilt.value,
            Some(Value::Tuple(tuple))
                if matches!(tuple.elements.as_slice(), [Value::I64(101), Value::F64(value)] if *value == 1.5)
        ));
        assert_ne!(session.last_vm_build_nanos(), Some(0));

        let zero_arg_caller_def =
            session.eval("call_user_collision_zero_9784() = __lambda_repl_1_0(1)");
        assert!(
            zero_arg_caller_def.success,
            "{:?}",
            zero_arg_caller_def.error
        );
        let caller = session.eval("call_user_collision_zero_9784()");
        assert!(caller.success, "{:?}", caller.error);
        assert!(
            matches!(caller.value, Some(Value::I64(101))),
            "unexpected caller result: {:?}",
            caller.value
        );
    });
}

/// A dispatch miss in the outer half of a composed callable is the same
/// catchable MethodError as an ordinary dynamic call. The compose continuation
/// must not degrade that miss to an internal TypeError (Issue #9784).
#[test]
fn composed_outer_dispatch_miss_raises_method_error_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definitions = session.eval("only_string_9784(x::String)=x; inner_int_9784(x)=1");
        assert!(definitions.success, "{:?}", definitions.error);
        let caught = session.eval(
            "try; (only_string_9784 ∘ inner_int_9784)(0); global compose_method_error_9784 = false; catch e; global compose_method_error_9784 = e isa MethodError; end",
        );
        assert!(caught.success, "{:?}", caught.error);
        let result = session.eval("compose_method_error_9784");
        assert!(result.success, "{:?}", result.error);
        assert!(
            matches!(result.value, Some(Value::Bool(true))),
            "unexpected compose result: {:?}",
            result.value
        );
    });
}

/// Full rebuild merges the current input before prior definitions in the IR
/// vector. Canonically equivalent signatures must nevertheless use Julia
/// definition chronology for both runtime dispatch and reflection, rather than
/// treating the vector tail as the newest method (Issue #9784).
#[test]
fn canonical_reflection_winner_matches_runtime_after_rebuild_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval(
                    "canonical_rebuild_9784(x::T) where {T<:Number} = (x < 0 ? error(\"old\") : -1); saved_canonical_rebuild_9784 = canonical_rebuild_9784"
                )
                .success
        );

        let rebuilt = session.eval(
            "macro force_canonical_rebuild_9784(); :(1); end; canonical_rebuild_9784(x::Number) = 2",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert_ne!(session.last_vm_build_nanos(), Some(0));

        let result = session.eval(
            "(canonical_rebuild_9784(1), saved_canonical_rebuild_9784(1), Base.return_types(canonical_rebuild_9784, Tuple{Int64}) == Any[Int64], Base.infer_exception_type(canonical_rebuild_9784, Tuple{Number}) === Union{}, Base.infer_exception_type(saved_canonical_rebuild_9784, Tuple{Number}) === Union{})",
        );
        assert!(result.success, "{:?}", result.error);
        assert!(
            matches!(
                result.value,
                Some(Value::Tuple(ref tuple))
                    if matches!(
                        tuple.elements.as_slice(),
                        [
                            Value::I64(2),
                            Value::I64(2),
                            Value::Bool(true),
                            Value::Bool(true),
                            Value::Bool(true),
                        ]
                    )
            ),
            "unexpected canonical chronology result: {:?}",
            result.value
        );
    });
}

/// Static HOF lowering must resolve a FunctionRef's exact helper provenance
/// before consulting the public method table. Otherwise a same-named source
/// generic hijacks `ntuple`'s raw function index (Issue #9784).
#[test]
fn static_ntuple_helper_does_not_resolve_to_same_named_source_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("__lambda_repl_1_0(x) = x + 100").success);

        let result = session.eval("ntuple(x -> x + 1, 2)");
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(
            result.value,
            Some(Value::Tuple(tuple))
                if matches!(tuple.elements.as_slice(), [Value::I64(2), Value::I64(3)])
        ));
    });
}

/// `sprint` is another static HOF instruction: its callable operand must keep
/// the exact lowered helper provenance instead of consulting a same-named
/// Julia-visible source generic (Issue #9784).
#[test]
fn sprint_helper_does_not_resolve_to_same_named_source_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval("__lambda_repl_1_0(io, x) = print(io, \"wrong\")")
                .success
        );

        let result = session.eval("sprint((io, x) -> print(io, x + 1), 1) == \"2\"");
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// Empty-generator element inference must resolve the callable from the
/// original FunctionRef, including its span/provenance. Reconstructing a span-0
/// reference by name lets an unrelated source generic dictate the result array
/// type (Issue #9784).
#[test]
fn empty_generator_eltype_uses_exact_helper_provenance_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("__lambda_repl_1_0(x) = 1.5").success);

        let result = session.eval("eltype(collect(Base.Generator(x -> x + 1, Int[]))) === Int64");
        assert!(result.success, "{:?}", result.error);
        assert!(matches!(result.value, Some(Value::Bool(true))));
    });
}

/// The same provenance boundary holds when the helper exists first and a user
/// later defines the exact same spelling/signature. The source method becomes
/// the public generic while the already-carried helper value remains callable.
#[test]
fn later_user_generic_does_not_replace_carried_lambda_helper_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let generated = session.eval("saved_reverse_collision_9784 = identity(x -> x + 1)");
        assert!(generated.success, "{:?}", generated.error);

        let user = session.eval("__lambda_repl_0_0(x) = x + 100");
        assert!(user.success, "{:?}", user.error);

        let rebuilt = session.eval(
            "macro force_reverse_collision_rebuild_9784(); :(1); end; (__lambda_repl_0_0(1), saved_reverse_collision_9784(1))",
        );
        assert!(rebuilt.success, "{:?}", rebuilt.error);
        assert!(matches!(
            rebuilt.value,
            Some(Value::Tuple(tuple))
                if matches!(tuple.elements.as_slice(), [Value::I64(101), Value::I64(2)])
        ));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// A helper with a differing signature must not leak as an overload of a user
/// generic that merely shares its spelling.
#[test]
fn helper_signature_is_not_public_user_overload_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(
            session
                .eval("saved_private_overload_9784 = identity(x -> x + 1)")
                .success
        );
        assert!(session.eval("__lambda_repl_0_0() = 77").success);

        let source = session.eval("__lambda_repl_0_0()");
        assert!(source.success, "{:?}", source.error);
        assert!(matches!(source.value, Some(Value::I64(77))));

        let hidden = session.eval("__lambda_repl_0_0(1)");
        assert!(
            !hidden.success,
            "private helper leaked as a source overload: {:?}",
            hidden.value
        );

        let helper = session.eval("saved_private_overload_9784(1)");
        assert!(helper.success, "{:?}", helper.error);
        assert!(matches!(helper.value, Some(Value::I64(2))));
    });
}

/// Generated-name spelling is not generated-function provenance. A user source
/// method with the same prefix must remain marker-gated and therefore cannot
/// become visible when execution throws before its definition (Issue #9784).
#[test]
fn helper_like_user_name_remains_source_ordered_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("helper_name_warm_9784 = 1").success);

        let failed = session.eval("error(\"stop\"); __gen_body_0(x) = x + 1");
        assert!(!failed.success);
        assert!(session.has_live_vm());
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let hidden = session.eval("isdefined(Main, :__gen_body_0)");
        assert!(hidden.success, "{:?}", hidden.error);
        assert!(matches!(hidden.value, Some(Value::Bool(false))));

        let mut defined_session = new_session();
        assert!(
            defined_session
                .eval("helper_name_live_warm_9784 = 1")
                .success
        );
        let defined = defined_session.eval("__gen_body_0(x) = x + 1");
        assert!(defined.success, "{:?}", defined.error);
        assert_eq!(defined_session.last_vm_build_nanos(), Some(0));
        let call = defined_session.eval("__gen_body_0(41)");
        assert!(call.success, "{:?}", call.error);
        assert!(matches!(call.value, Some(Value::I64(42))));
        assert_eq!(defined_session.last_vm_build_nanos(), Some(0));
    });
}

/// Capture analysis must use structural lowering provenance, not generated-name
/// prefixes. A user method named like a generator helper still reads globals in
/// the ordinary source-method environment before and after a full rebuild.
#[test]
fn helper_like_user_name_reads_global_normally_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("helper_like_global_9784 = 10").success);
        assert!(
            session
                .eval("__gen_body_user_9784(x) = x + helper_like_global_9784")
                .success
        );

        let before = session.eval("__gen_body_user_9784(1)");
        assert!(before.success, "{:?}", before.error);
        assert!(matches!(before.value, Some(Value::I64(11))));

        let after = session.eval(
            "macro force_helper_like_user_rebuild_9784(); :(1); end; __gen_body_user_9784(1)",
        );
        assert!(after.success, "{:?}", after.error);
        assert!(matches!(after.value, Some(Value::I64(11))));
        assert_ne!(session.last_vm_build_nanos(), Some(0));
    });
}

/// Do-block and generator lowering produce the same marker-less callable class
/// as arrow lambdas. Both remain on the held VM, including when their generated
/// names restart in successive parser inputs (Issue #9784).
#[test]
fn do_block_and_generator_helpers_install_on_live_vm_9784() {
    run_with_large_stack(|| {
        let mut generator_only = new_session();
        assert!(generator_only.eval("generator_only_warm_9784 = 1").success);
        let generator = generator_only.eval("sum(x + 1 for x in 1:3) == 9");
        assert!(generator.success, "{:?}", generator.error);
        assert!(matches!(generator.value, Some(Value::Bool(true))));
        assert_eq!(
            generator_only.last_vm_build_nanos(),
            Some(0),
            "a standalone generator helper must install on the live VM"
        );

        let mut session = new_session();
        assert!(session.eval("helper_family_warm_9784 = 1").success);

        let do_block = session.eval("(map([1, 2]) do x; x + 2; end) == [3, 4]");
        assert!(do_block.success, "{:?}", do_block.error);
        assert!(matches!(do_block.value, Some(Value::Bool(true))));
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "a do-block helper must install live"
        );

        let generator = session.eval("sum(x + 1 for x in 1:3) == 9");
        assert!(generator.success, "{:?}", generator.error);
        assert!(matches!(generator.value, Some(Value::Bool(true))));
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "a generator after a do-block helper must still install live"
        );

        let filtered = session.eval("sum(x for x in 1:5 if isodd(x)) == 9");
        assert!(filtered.success, "{:?}", filtered.error);
        assert!(matches!(filtered.value, Some(Value::Bool(true))));
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "a filtered generator body/predicate pair must install live"
        );
    });
}

/// A batch with a definition after the throwing statement commits only the
/// source-order prefix reached before the error. The dormant suffix stays
/// undefined both on the live VM and after a forced full fallback (Issue #9784).
#[test]
fn partial_definition_prefix_survives_error_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let old = session.eval("old_partial_snapshot9784() = 40");
        assert!(old.success, "{:?}", old.error);

        let failed = session.eval(
            "before_partial_snapshot9784() = 41; error(\"partial definition boom\"); after_partial_snapshot9784() = 99",
        );
        assert!(!failed.success, "the appended main must report its error");
        assert!(
            session.has_live_vm(),
            "the reached definition prefix must retain the live VM"
        );

        let reached = session.eval("before_partial_snapshot9784() + 1");
        assert!(reached.success, "{:?}", reached.error);
        assert!(matches!(reached.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let dormant = session.eval("isdefined(Main, :after_partial_snapshot9784)");
        assert!(dormant.success, "{:?}", dormant.error);
        assert!(matches!(dormant.value, Some(Value::Bool(false))));

        let forced_fallback = session.eval(
            "let local_partial9784 = before_partial_snapshot9784\n  local_partial9784() + 1\nend",
        );
        assert!(forced_fallback.success, "{:?}", forced_fallback.error);
        assert!(matches!(forced_fallback.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let dormant_after_fallback = session.eval("isdefined(Main, :after_partial_snapshot9784)");
        assert!(
            dormant_after_fallback.success,
            "{:?}",
            dormant_after_fallback.error
        );
        assert!(matches!(
            dormant_after_fallback.value,
            Some(Value::Bool(false))
        ));
    });
}

/// The compiler checkpoint is method-granular: when two declarations extend
/// the same new generic, only the reached signature is visible and replayed.
#[test]
fn partial_definition_same_generic_keeps_only_reached_method_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let old = session.eval("old_partial_overload9784() = 0");
        assert!(old.success, "{:?}", old.error);

        let failed = session.eval(
            "partial_overload9784(x::Int) = x + 1; error(\"partial overload boom\"); partial_overload9784(x::Float64) = x + 2.0",
        );
        assert!(!failed.success);
        assert!(session.has_live_vm());

        let reached = session.eval("partial_overload9784(41)");
        assert!(reached.success, "{:?}", reached.error);
        assert!(matches!(reached.value, Some(Value::I64(42))));
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let dormant = session.eval("partial_overload9784(1.0)");
        assert!(
            !dormant.success,
            "the unreached Float64 method must stay hidden"
        );
        assert!(
            dormant
                .error
                .as_deref()
                .is_some_and(|error| error.contains("MethodError")),
            "{:?}",
            dormant.error
        );

        let forced_fallback = session.eval(
            "let local_partial_overload9784 = partial_overload9784\n  local_partial_overload9784(41)\nend",
        );
        assert!(forced_fallback.success, "{:?}", forced_fallback.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));

        let dormant_after_fallback = session.eval("partial_overload9784(1.0)");
        assert!(!dormant_after_fallback.success);
        assert!(dormant_after_fallback
            .error
            .as_deref()
            .is_some_and(|error| error.contains("MethodError")));
    });
}

/// A reached function body may refer forward to an unreached definition. The
/// dormant body must not become callable until a later eval actually defines
/// that generic, at which point ordinary late binding repairs the call.
#[test]
fn partial_definition_forward_reference_stays_late_bound_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let old = session.eval("old_partial_forward9784() = 0");
        assert!(old.success, "{:?}", old.error);

        let failed = session.eval(
            "before_partial_forward9784() = later_partial_forward9784(); error(\"partial forward boom\"); later_partial_forward9784() = 99",
        );
        assert!(!failed.success);
        assert!(session.has_live_vm());

        let unresolved = session.eval("before_partial_forward9784()");
        assert!(!unresolved.success);
        assert!(unresolved
            .error
            .as_deref()
            .is_some_and(|error| error.contains("UndefVarError")));

        let define_later = session.eval("later_partial_forward9784() = 99");
        assert!(define_later.success, "{:?}", define_later.error);
        let repaired = session.eval("before_partial_forward9784()");
        assert!(repaired.success, "{:?}", repaired.error);
        assert!(matches!(repaired.value, Some(Value::I64(99))));
    });
}

/// A plain-looking call can mutate the live definition world through runtime
/// `@eval`. This slice cannot advance the compiler snapshot for that hidden
/// definition, so recovery must reject the changed VM.
#[test]
fn runtime_eval_definition_error_drops_unrepresentable_live_world_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let definition = session.eval(
            "runtime_eval_writer9784() = (@eval runtime_eval_created9784() = 1; error(\"runtime eval boom\"))",
        );
        assert!(definition.success, "{:?}", definition.error);

        let failed = session.eval("runtime_eval_writer9784()");
        assert!(
            !failed.success,
            "the runtime eval call must report its error"
        );
        assert!(
            !session.has_live_vm(),
            "a runtime-mutated definition world must not outlive its stale compiler snapshot"
        );
    });
}

/// Determinism pin for the persistent model: two independent `Persistent`
/// sessions driven through the migration-seam corpus must observe identically at
/// every step. This guards against nondeterminism introduced by the value-carry /
/// re-seed machinery; golden checks alone would otherwise exercise only one
/// persistent session.
#[test]
fn differential_migration_seams_persistent_is_deterministic_9199() {
    run_with_large_stack(|| {
        let corpus = load_diff_corpus("repl_differential/migration_seams_9199.toml");
        let actions: Vec<Action> = corpus.steps.iter().map(DiffStep::to_action).collect();
        // Empty divergence set: two identical models must agree at every step.
        run_deterministic_pairs(
            &format!("{}[persistent-determinism]", corpus.name),
            &actions,
        );
    });
}

/// `reset()` must return the session to observational equivalence with a fresh
/// one for ordinary user constructs: after building state and resetting, the same
/// probe sequence produces identical observations to a brand-new session.
///
/// NOTE: the probe deliberately avoids `InteractiveUtils`-provided names. Current
/// `reset()` clears `usings` without restoring the default `InteractiveUtils`
/// auto-import (an asymmetry called out in the #9199 ADR as an S3 exit criterion),
/// so a stricter "reset == new including auto-imports" check would fail on today's
/// model. This keeps the pin green now while documenting the gap.
#[test]
fn reset_is_observationally_equivalent_to_fresh_9199() {
    run_with_large_stack(|| {
        // This is the structural guarantee behind #9193 (reset()-leak): globals
        // live in the carried binding snapshot, so dropping it on reset() cannot
        // leave a scalar shadow (`first = true`) or stale type hint behind.
        {
            let probe = [
                Action::Eval("p9199 = 100".to_string()),
                Action::Eval("q9199(x) = x * 2".to_string()),
                Action::Eval("q9199(p9199)".to_string()),
                Action::Eval("[p9199, 1, 2]".to_string()),
            ];

            // Fresh session: run only the probe.
            let mut fresh = new_session();
            let fresh_obs: Vec<Observation> =
                probe.iter().map(|a| observe(&mut fresh, a)).collect();

            // Dirtied session: build unrelated state, reset, then run the probe.
            let mut dirtied = new_session();
            for setup in [
                "junk9199 = 999",
                "struct Junk9199\n  a::Int\nend",
                "jk9199 = Junk9199(5)",
                "q9199(x) = x + 1000", // a conflicting definition of the probe's name
                "first = true",        // a scalar shadowing a Base generic (the #9193 shape)
                "using Printf",
            ] {
                let _ = observe(&mut dirtied, &Action::Eval(setup.to_string()));
            }
            observe(&mut dirtied, &Action::Reset);
            let reset_obs: Vec<Observation> =
                probe.iter().map(|a| observe(&mut dirtied, a)).collect();

            assert_eq!(
                reset_obs, fresh_obs,
                "a reset session diverged from a fresh session on the probe sequence; \
                 reset() must drop all prior state (Issue #9199 / #9193 exit criterion)"
            );
        }
    });
}

/// S6 (Issue #9199): world-age at REPL eval granularity — a method redefined in
/// a later eval must NOT be applied retroactively to an earlier eval's
/// already-computed result. This is the sharpest world-age invariant
/// (`julia/src/gf.c`: a call sees only methods defined in worlds ≤ its own; a
/// redefinition bumps the world counter and never rewrites a value produced in an
/// earlier world). It holds because the REPL eval boundary is the world boundary —
/// the earlier eval's call already ran and stored its result
/// before the later eval redefines. The corpus rows exercise call-before /
/// call-after visibility; this pins the complementary "the saved value stays put"
/// half. Values verified against upstream `julia` (11 / 110 / 11).
#[test]
fn worldage_redefinition_is_not_retroactive_across_evals_9199_s6() {
    run_with_large_stack(|| {
        {
            let mut s = new_session();
            // eval 1: define the method.
            assert!(s.eval("wg6s6(x) = x + 1").success, "initial def");
            // eval 2: capture the method's result into a global BEFORE any redef.
            // (An assignment eval does not itself echo a display value in the REPL,
            // so the pre-redef result is asserted below via a bare reference read.)
            assert!(
                s.eval("saved6s6 = wg6s6(10)").success,
                "pre-redef call into a global must succeed"
            );
            // eval 3: redefine the method (same signature).
            assert!(s.eval("wg6s6(x) = x + 100").success, "redef");
            // eval 4: a NEW call site sees the redefinition (current world).
            let after = s.eval("wg6s6(10)");
            assert_eq!(
                display_of(&after, "wg6s6(10)").as_deref(),
                Some("110"),
                "post-redef call must see the redefined method"
            );
            // eval 5: the earlier eval's already-computed global is UNCHANGED — the
            // redefinition is not retroactive (the world-age invariant).
            let reread = s.eval("saved6s6");
            assert_eq!(
                display_of(&reread, "saved6s6").as_deref(),
                Some("11"),
                "a later redefinition must NOT retroactively change an \
                 earlier eval's already-computed result (Issue #9199 S6 world-age)"
            );
        }
    });
}

/// Issue #9784 Slice 2: ordinary user-method extension/replacement must update
/// the one live VM. A caller compiled in an earlier eval is refreshed once and
/// carries source-ordered candidates for two later same-input replacements, so
/// calls between definition markers observe the matching world. Values were
/// verified against upstream Julia 1.12 (1.5 / 101 / 1001 / 2001 / 3001).
#[test]
fn method_mutations_stay_on_one_live_vm_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("method_delta_9784(x::Int64) = x + 1").success);

        let extension = session.eval("method_delta_9784(x::Float64) = x + 0.5");
        assert!(extension.success, "{:?}", extension.error);
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "a method extension must update the held live VM"
        );
        let float_call = session.eval("method_delta_9784(1.0)");
        assert_eq!(
            display_of(&float_call, "method_delta_9784(1.0)").as_deref(),
            Some("1.5")
        );

        let replacement = session.eval("method_delta_9784(x::Int64) = x + 100");
        assert!(replacement.success, "{:?}", replacement.error);
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "a same-signature replacement must update the held live VM"
        );
        let replaced_call = session.eval("method_delta_9784(1)");
        assert_eq!(
            display_of(&replaced_call, "method_delta_9784(1)").as_deref(),
            Some("101")
        );

        assert!(
            session
                .eval("method_caller_9784(x) = method_delta_9784(x)")
                .success
        );
        let before_refresh = session.eval("method_caller_9784(1)");
        assert_eq!(
            display_of(&before_refresh, "method_caller_9784(1)").as_deref(),
            Some("101")
        );
        let refreshed = session.eval("method_delta_9784(x::Int64) = x + 1000");
        assert!(refreshed.success, "{:?}", refreshed.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        let after_refresh = session.eval("method_caller_9784(1)");
        assert_eq!(
            display_of(&after_refresh, "method_caller_9784(1)").as_deref(),
            Some("1001")
        );

        let ordered = session.eval(
            "before_9784 = method_caller_9784(1); method_delta_9784(x::Int64) = x + 2000; middle_9784 = method_caller_9784(1); method_delta_9784(x::Int64) = x + 3000; after_9784 = method_caller_9784(1)",
        );
        assert!(ordered.success, "{:?}", ordered.error);
        assert_eq!(
            session.last_vm_build_nanos(),
            Some(0),
            "multiple source-ordered replacements must stay on the live VM"
        );
        for (name, expected) in [
            ("before_9784", "1001"),
            ("middle_9784", "2001"),
            ("after_9784", "3001"),
        ] {
            let value = session.eval(name);
            assert_eq!(
                display_of(&value, name).as_deref(),
                Some(expected),
                "{name}"
            );
        }
    });
}

/// Issue #9784: `where` is structured method identity, not a reason to rebuild
/// the VM. Values are upstream Julia 1.12 goldens.
#[test]
fn parametric_method_mutations_stay_on_one_live_vm_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("where_warm_9784 = 1").success);

        let defined = session.eval("where_f_9784(x::T) where {T<:Integer} = x + one(T)");
        assert!(defined.success, "{:?}", defined.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert_eq!(
            display_of(&session.eval("where_f_9784(2)"), "where_f_9784(2)").as_deref(),
            Some("3")
        );

        let extension = session.eval("where_f_9784(x::T, y::T) where {T<:Integer} = x + y");
        assert!(extension.success, "{:?}", extension.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert_eq!(
            display_of(&session.eval("where_f_9784(2, 3)"), "where_f_9784(2, 3)",).as_deref(),
            Some("5")
        );

        assert!(
            session
                .eval("where_caller_9784(x) = where_f_9784(x)")
                .success
        );
        let replacement = session.eval("where_f_9784(x::T) where {T<:Integer} = x + T(100)");
        assert!(replacement.success, "{:?}", replacement.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert_eq!(
            display_of(
                &session.eval("where_caller_9784(2)"),
                "where_caller_9784(2)",
            )
            .as_deref(),
            Some("102")
        );
    });
}

/// Issue #9784: keyword metadata and specializations publish with their source
/// method marker. Values are upstream Julia 1.12 goldens.
#[test]
fn keyword_method_mutations_stay_on_one_live_vm_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("kw_warm_9784 = 1").success);

        let defined = session.eval("kw_f_9784(x; scale=1, bias=0) = x * scale + bias");
        assert!(defined.success, "{:?}", defined.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        for (expr, expected) in [
            ("kw_f_9784(2)", "2"),
            ("kw_f_9784(2; scale=3, bias=4)", "10"),
            (
                "kw_nt_9784=(scale=3,bias=4); kw_f_9784(2; kw_nt_9784...)",
                "10",
            ),
        ] {
            assert_eq!(
                display_of(&session.eval(expr), expr).as_deref(),
                Some(expected)
            );
        }

        let vararg = session.eval("kwvar_f_9784(xs...; scale=1) = sum(xs) * scale");
        assert!(vararg.success, "{:?}", vararg.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert_eq!(
            display_of(
                &session.eval("kwvar_f_9784(1,2,3; scale=2)"),
                "kwvar_f_9784(1,2,3; scale=2)",
            )
            .as_deref(),
            Some("12")
        );

        assert!(
            session
                .eval("kw_caller_9784(x) = kw_f_9784(x; scale=2)")
                .success
        );
        let replacement = session.eval("kw_f_9784(x; scale=1, bias=0) = x * scale + bias + 100");
        assert!(replacement.success, "{:?}", replacement.error);
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert_eq!(
            display_of(&session.eval("kw_caller_9784(2)"), "kw_caller_9784(2)",).as_deref(),
            Some("104")
        );
    });
}

/// Issue #9784: qualified caller refresh uses the exact method prefix visible
/// at each source marker, in both caller-before and caller-after orders.
#[test]
fn qualified_method_refresh_uses_each_marker_snapshot_9784() {
    run_with_large_stack(|| {
        let mut later = new_session();
        assert!(later.eval("qf_later_9784(x; k=1) = x + k").success);
        assert!(
            later
                .eval("qc_later_9784(x) = qf_later_9784(x; k=1)")
                .success
        );
        let result = later.eval(
            "qf_later_9784(x; k=1) = x + k + 1000; qprior_9784 = qc_later_9784(0); qc_later_9784(x) = qf_later_9784(x; k=1) + 10; qnew_9784 = qc_later_9784(0)",
        );
        assert!(result.success, "{:?}", result.error);
        assert_eq!(later.last_vm_build_nanos(), Some(0));
        for (name, expected) in [("qprior_9784", "1001"), ("qnew_9784", "1011")] {
            assert_eq!(
                display_of(&later.eval(name), name).as_deref(),
                Some(expected)
            );
        }

        let mut earlier = new_session();
        assert!(earlier.eval("qf_earlier_9784(x; k=1) = x + k").success);
        assert!(
            earlier
                .eval("qc_earlier_9784(x) = qf_earlier_9784(x; k=1)")
                .success
        );
        let result = earlier.eval(
            "qc_earlier_9784(x) = qf_earlier_9784(x; k=1) + 10; qbefore_9784 = qc_earlier_9784(0); qf_earlier_9784(x::Int; k=1) = x + k + 2000; qafter_9784 = qc_earlier_9784(0)",
        );
        assert!(
            result.success,
            "error={:?}, vm_build={:?}, live_vm={}",
            result.error,
            earlier.last_vm_build_nanos(),
            earlier.has_live_vm()
        );
        assert_eq!(earlier.last_vm_build_nanos(), Some(0));
        for (name, expected) in [("qbefore_9784", "11"), ("qafter_9784", "2011")] {
            assert_eq!(
                display_of(&earlier.eval(name), name).as_deref(),
                Some(expected)
            );
        }
    });
}

/// Issue #9784: a combined `where`+keyword replacement and its caller refresh
/// commit at one marker; an extension after a catchable error stays dormant.
#[test]
fn qualified_method_error_commits_only_reached_prefix_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("mixed_warm_9784 = 1").success);
        assert!(
            session
                .eval("mixed_f_9784(x::T; k::T=one(T)) where {T<:Integer} = x + k")
                .success
        );
        assert!(
            session
                .eval("mixed_caller_9784(x) = mixed_f_9784(x; k=x)")
                .success
        );

        let failed = session.eval(
            "mixed_f_9784(x::T; k::T=one(T)) where {T<:Integer} = x + k + T(100); mixed_before_error_9784 = mixed_caller_9784(2); error(\"stop qualified prefix\"); mixed_f_9784(x::T, y::T; k=one(T)) where {T<:Integer} = x + y + k",
        );
        assert!(!failed.success);
        assert!(session.has_live_vm());
        assert_eq!(session.last_vm_build_nanos(), Some(0));
        assert_eq!(
            display_of(
                &session.eval("mixed_before_error_9784"),
                "mixed_before_error_9784",
            )
            .as_deref(),
            Some("104")
        );
        assert_eq!(
            display_of(
                &session.eval("mixed_caller_9784(3)"),
                "mixed_caller_9784(3)",
            )
            .as_deref(),
            Some("106")
        );
        // Issue #10813 separately tracks top-level no-method failures that are
        // reported at compile time; `applicable` is the direct method-presence
        // oracle for this reached-prefix transaction.
        let absent = session.eval("applicable(mixed_f_9784, 1, 2)");
        assert!(absent.success, "{:?}", absent.error);
        assert_eq!(
            display_of(&absent, "applicable(mixed_f_9784, 1, 2)").as_deref(),
            Some("false")
        );

        let closure = session.eval("get!(() -> 42, Dict{Symbol,Any}(), :x)");
        assert!(closure.success, "{:?}", closure.error);
    });
}

/// Issue #9784: caller refresh is planned at each source marker, not from the
/// input's final method set. A caller redefined later still needs its prior body
/// refreshed before that later marker; a caller redefined earlier needs that
/// new body refreshed by a subsequent callee mutation. Values were verified
/// against upstream Julia 1.12: `(1000, 1010)` and `(11, 2010)`.
#[test]
fn method_mutation_refresh_uses_each_marker_snapshot_9784() {
    run_with_large_stack(|| {
        let mut later_caller = new_session();
        assert!(later_caller.eval("snapshot_f_9784(x) = x + 1").success);
        assert!(
            later_caller
                .eval("snapshot_c_9784(x) = snapshot_f_9784(x)")
                .success
        );
        let later = later_caller.eval(
            "snapshot_f_9784(x) = x + 1000; prior_caller_9784 = snapshot_c_9784(0); snapshot_c_9784(x) = snapshot_f_9784(x) + 10; new_caller_9784 = snapshot_c_9784(0)",
        );
        assert!(later.success, "{:?}", later.error);
        assert_eq!(later_caller.last_vm_build_nanos(), Some(0));
        for (name, expected) in [("prior_caller_9784", "1000"), ("new_caller_9784", "1010")] {
            let value = later_caller.eval(name);
            assert_eq!(
                display_of(&value, name).as_deref(),
                Some(expected),
                "{name}"
            );
        }
        let persisted = later_caller.eval("snapshot_probe_9784() = snapshot_c_9784(0)");
        assert!(persisted.success, "{:?}", persisted.error);
        assert_eq!(later_caller.last_vm_build_nanos(), Some(0));
        let persisted_call = later_caller.eval("snapshot_probe_9784()");
        assert_eq!(
            display_of(&persisted_call, "snapshot_probe_9784()").as_deref(),
            Some("1010"),
            "the next definition delta must compile against marker-final methods"
        );

        let mut earlier_caller = new_session();
        assert!(earlier_caller.eval("snapshot_f2_9784(x) = x + 1").success);
        assert!(
            earlier_caller
                .eval("snapshot_c2_9784(x) = snapshot_f2_9784(x)")
                .success
        );
        let earlier = earlier_caller.eval(
            "snapshot_c2_9784(x) = snapshot_f2_9784(x) + 10; before_callee_9784 = snapshot_c2_9784(0); snapshot_f2_9784(x) = x + 2000; after_callee_9784 = snapshot_c2_9784(0)",
        );
        assert!(earlier.success, "{:?}", earlier.error);
        assert_eq!(earlier_caller.last_vm_build_nanos(), Some(0));
        for (name, expected) in [("before_callee_9784", "11"), ("after_callee_9784", "2010")] {
            let value = earlier_caller.eval(name);
            assert_eq!(
                display_of(&value, name).as_deref(),
                Some(expected),
                "{name}"
            );
        }
    });
}

/// Issue #9784: module-owned callers that import a Main generic must not retain
/// a stale direct target when that generic is replaced, even when the compiler
/// conservatively chooses a fresh fallback. Upstream Julia 1.12 returns `1001`.
#[test]
fn method_mutation_reaches_importing_module_callers_9784() {
    run_with_large_stack(|| {
        let mut session = new_session();
        assert!(session.eval("module_f_9784(x) = x + 1").success);
        assert!(session
            .eval("module MCaller9784\nimport Main: module_f_9784\ncall(x) = module_f_9784(x)\nend")
            .success);

        let replacement = session.eval("module_f_9784(x) = x + 1000");
        assert!(replacement.success, "{:?}", replacement.error);
        let call = session.eval("MCaller9784.call(1)");
        assert_eq!(
            display_of(&call, "MCaller9784.call(1)").as_deref(),
            Some("1001")
        );
    });
}

/// Issue #9786 (LV6 flip blocker): a NamedTuple global assigned in one eval and
/// **destructured in a later eval** must match the upstream-reviewed values.
/// Under Persistent, `F` is a value-carried runtime NamedTuple and the delta
/// compile of `U, S, V = F` emits `TupleUnpack` on an opaque RHS; before the fix
/// that hit an `InternalError: Expected Tuple, got NamedTuple(...)` because
/// `TupleUnpack` had no `Value::NamedTuple` arm. A NamedTuple destructures to its
/// field values in declaration order (`x,y,z = nt` binds `x=nt[1]`, …), matching
/// upstream `julia` (values verified: U[1]==1.0, S[2]==4.0). This drives the exact
/// cross-eval sequence through the production model and pins the concrete reads.
#[test]
fn namedtuple_cross_eval_destructure_matches_upstream_9786() {
    run_with_large_stack(|| {
        let actions = [
            Action::Eval("F = (U = [1.0, 2.0], S = [3.0, 4.0], V = [5.0, 6.0])".to_string()),
            Action::Eval("U, S, V = F".to_string()),
            Action::Eval("U[1]".to_string()),
            Action::Eval("S[2]".to_string()),
        ];

        let mut persistent = new_session();
        let persistent_obs: Vec<Observation> = actions
            .iter()
            .map(|a| observe(&mut persistent, a))
            .collect();

        // The cross-eval destructure must succeed (the bug was a hard InternalError).
        assert!(
            persistent_obs[1].success,
            "`U, S, V = F` (cross-eval) must succeed (Issue #9786); err={:?}",
            persistent_obs[1].error
        );
        // Concrete raw values under Persistent, verified against upstream julia 1.12.
        // Assert the raw `Value` (not the coarse display projection, which renders
        // an F64 `1.0` as "1"): the destructured `U[1]` / `S[2]` must be the exact
        // NamedTuple field values. `U`/`S` are still bound on the persistent session.
        let u1 = persistent.eval("U[1]");
        assert!(
            matches!(u1.value, Some(Value::F64(v)) if (v - 1.0).abs() < 1e-9),
            "U[1] after NamedTuple destructure must be 1.0 under Persistent (Issue #9786), got {:?}",
            u1.value
        );
        let s2 = persistent.eval("S[2]");
        assert!(
            matches!(s2.value, Some(Value::F64(v)) if (v - 4.0).abs() < 1e-9),
            "S[2] after NamedTuple destructure must be 4.0 under Persistent (Issue #9786), got {:?}",
            s2.value
        );
    });
}

/// S6 (Issue #9199): the direct intra-eval / script-mode dual (#9400) now uses
/// a source-visible method table for top-level call sites. A same-signature
/// redefinition placed textually between two direct calls in ONE eval resolves
/// the first save to the old method and the second save to the new method,
/// matching upstream's 12 result under both sjulia models. Calls that enter an
/// earlier-defined function body still need runtime world propagation (Issue
/// #9650).
#[test]
fn worldage_intra_eval_9400_is_identical_across_models_9199_s6() {
    run_with_large_stack(|| {
        // One eval defines wq6s6()=1, saves it into sa6s6, redefines wq6s6()=2,
        // saves into sb6s6. Source-visible top-level dispatch gives sa6s6 == 1
        // and sb6s6 == 2, matching upstream. Read the two globals back via bare
        // references (an assignment eval does not echo a display).
        let input = "wq6s6() = 1\nsa6s6 = wq6s6()\nwq6s6() = 2\nsb6s6 = wq6s6()";
        {
            let mut s = new_session();
            assert!(s.eval(input).success, "input must run");
            let sa = s.eval("sa6s6");
            assert_eq!(
                display_of(&sa, "sa6s6").as_deref(),
                Some("1"),
                "the earlier direct call must see the method visible at \
                 its source point (Issue #9400)"
            );
            let sb = s.eval("sb6s6");
            assert_eq!(
                display_of(&sb, "sb6s6").as_deref(),
                Some("2"),
                "the later save sees the redefinition"
            );
        }
    });
}

/// P2 regression (Issue #9199 review r3536211269): a hard-scope `let` in an
/// assignment's INDEX / KEY expression — `a[let x = 99; 1 end] = v` /
/// `d[let x = 99; :k end] = v` — must be detected as a hard scope so the input is
/// NOT taken as a live-append/parking delta. If it slipped through (the pre-fix
/// bug scanned only the RHS `value`), the fresh-delta path's `ForgetLetLocals(["x"])`
/// would clear the live global `x`'s frame-0 slot at block exit, and — because
/// `input_has_hard_scope` was `false` — that CORRUPTED VM was parked for the next
/// live delta, which then read `x` back as `UndefVarError`. The shadow name here
/// deliberately matches the prior global so the clear targets a live slot.
///
/// Asserted against upstream-reviewed goldens and an independent-session
/// determinism run: the global survives (reads back `5`, never an error), the mutation lands
/// (`[7, 0, 0]` / `d[:k] == 42`), matching upstream `julia` (`5` / `[7, 0, 0]` /
/// `5` / `42`, verified).
#[test]
fn index_key_hard_scope_let_does_not_corrupt_frame0_global_9199() {
    run_with_large_stack(|| {
        // (1) Array index `let` shadowing the live global `x`.
        let array_seq = [
            Action::Eval("xr2 = 5".to_string()),
            Action::Eval("ar2 = [0, 0, 0]".to_string()),
            // The index is a hard-scope `let` whose local shadows the global `xr2`.
            Action::Eval("ar2[let xr2 = 99; 1 end] = 7".to_string()),
            // The next eval is the "next live delta": it must read the UNCORRUPTED
            // global, not an UndefVarError from a cleared frame-0 slot.
            Action::Eval("xr2".to_string()),
            // Read the mutated element as a SCALAR (the harness projects scalars
            // cleanly; a bare array reference dumps its carrier debug form).
            Action::Eval("ar2[1]".to_string()),
        ];
        // Golden-checked and deterministic across independent persistent sessions.
        let obs = run_persistent("index-let-array-9199", &array_seq);
        assert_eq!(
            display_of_obs(&obs[3]),
            Some("5"),
            "the global `xr2` must survive the index-`let` shadow (frame-0 not cleared)"
        );
        assert!(
            obs[3].success && obs[3].error.is_none(),
            "reading `xr2` after the index-`let` assign must not error (got {:?})",
            obs[3].error
        );
        assert_eq!(
            obs[4].display.as_deref(),
            Some("7"),
            "the indexed assignment must land at position 1 (ar2[1] == 7)"
        );

        // (2) Dict key `let` shadowing the live global `x`.
        let dict_seq = [
            Action::Eval("xr2d = 5".to_string()),
            Action::Eval("dr2 = Dict(:k => 0)".to_string()),
            Action::Eval("dr2[let xr2d = 99; :k end] = 42".to_string()),
            Action::Eval("xr2d".to_string()),
            Action::Eval("dr2[:k]".to_string()),
        ];
        let obs = run_persistent("index-let-dict-9199", &dict_seq);
        assert_eq!(
            display_of_obs(&obs[3]),
            Some("5"),
            "the global `xr2d` must survive the Dict-key `let` shadow"
        );
        assert!(
            obs[3].success && obs[3].error.is_none(),
            "reading `xr2d` after the key-`let` assign must not error (got {:?})",
            obs[3].error
        );
        assert_eq!(
            obs[4].display.as_deref(),
            Some("42"),
            "the Dict-key assignment must land at key :k"
        );
    });
}

/// Small helper: an observation's display string (already computed in `observe`).
fn display_of_obs(obs: &Observation) -> Option<&str> {
    obs.display.as_deref()
}

/// Sanity: the two projection helpers agree with hand-computed expectations, so a
/// regression in the projection itself (not the VM) is caught locally.
#[test]
fn projection_helpers_are_stable_9199() {
    let heap: &[StructInstance] = &[];
    assert_eq!(julia_type_repr(&Value::I64(1), heap), "Int64");
    assert_eq!(julia_type_repr(&Value::F64(1.0), heap), "Float64");
    assert_eq!(julia_type_repr(&Value::Bool(true), heap), "Bool");
    assert_eq!(julia_type_repr(&Value::Char('x'), heap), "Char");
    assert_eq!(
        julia_type_repr(&Value::str_new("hi".to_string()), heap),
        "String"
    );
    assert_eq!(
        classify_error("UndefVarError: `x` not defined"),
        "UndefVarError"
    );
    assert_eq!(
        classify_error("BoundsError: attempt to access"),
        "BoundsError"
    );
}
