//! Handler-table dispatch ↔ `match` dispatch parity harness (Issue #8562).
//!
//! Runs real compiled fixtures twice — once with the `SJULIA_HANDLER_TABLE=1`
//! gate off (production `match` dispatch) and once with it on (dispatch
//! through the function-pointer handler table) — and diffs the printed
//! output. The fixture sources were verified against upstream Julia
//! (`julia --startup-file=no`): fib(20) = 6765, calc_pi(100000) =
//! 3.1415826535897198, calc_pi_call(100000) = 3.1415826535897198,
//! lorenz_accum(100000) = -139963.64116703314.
//!
//! Only built under the `vm-handler-table` cargo feature (see `Cargo.toml`
//! `[[test]] required-features`); the default suite is unaffected.

use std::sync::Mutex;

use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

/// Serializes process-env manipulation across tests: nextest runs one process
/// per test, but plain `cargo test` shares the environment between threads.
static ENV_LOCK: Mutex<()> = Mutex::new(());

const GATE_ENV: &str = "SJULIA_HANDLER_TABLE";

/// Recursive benchmark (`fib`-class, Issue #8448 target list): exercises the
/// direct-call rows, I64 arithmetic/comparisons, and returns.
const FIB_SRC: &str = r#"
function fib(n::Int64)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

println(fib(20))
"#;

/// Loop benchmark (`calc_pi`-class): F64 arithmetic, slot loads/stores,
/// fused compare-and-branch forms.
const CALC_PI_SRC: &str = r#"
function calc_pi(n::Int64)
    acc = 0.0
    sign = 1.0
    k = 0
    while k < n
        acc = acc + sign / (2.0 * k + 1.0)
        sign = -sign
        k = k + 1
    end
    return 4.0 * acc
end

println(calc_pi(100000))
"#;

/// Loop with a user-function call per iteration: keeps the loop on the
/// per-instruction interpreter (no whole-loop executable block), so the
/// handler-table rows for the call family are exercised inside a hot loop.
const CALC_PI_CALL_SRC: &str = r#"
function pi_term(k::Int64)
    sign = 1.0 - 2.0 * (k % 2)
    return sign / (2.0 * k + 1.0)
end

function calc_pi_call(n::Int64)
    acc = 0.0
    k = 0
    while k < n
        acc = acc + pi_term(k)
        k = k + 1
    end
    return 4.0 * acc
end

println(calc_pi_call(100000))
"#;

/// Attractor-style Float64 loop (slot-heavy F64 arithmetic; the #8559
/// benchmark shape at a test-friendly iteration count).
const LORENZ_SRC: &str = r#"
function lorenz_accum(n::Int64)
    x = 1.0
    y = 1.0
    z = 1.0
    dt = 0.001
    acc = 0.0
    k = 0
    while k < n
        dx = 10.0 * (y - x)
        dy = x * (28.0 - z) - y
        dz = x * y - 2.6666666666666665 * z
        x = x + dt * dx
        y = y + dt * dy
        z = z + dt * dz
        acc = acc + x
        k = k + 1
    end
    return acc
end

println(lorenz_accum(100000))
"#;

struct RunResult {
    output: String,
    table_metrics: Option<(u64, u64)>,
}

fn run_source(src: &str) -> RunResult {
    let program = parse_and_lower_with_base_dir(src, None)
        .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
    let compiled = compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
    RunResult {
        output: vm.get_output().to_string(),
        table_metrics: vm.handler_table_metrics(),
    }
}

/// Run `src` with the gate off and on; the printed output must be identical,
/// match upstream Julia, and the gated run must serve dispatches from hot
/// table rows.
fn assert_gate_parity(name: &str, src: &str, expected_output: &str) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    std::env::remove_var(GATE_ENV);
    let off = run_source(src);
    assert_eq!(
        off.table_metrics, None,
        "{name}: gate off must never arm the handler table"
    );

    std::env::set_var(GATE_ENV, "1");
    let on = run_source(src);
    std::env::remove_var(GATE_ENV);

    assert_eq!(
        off.output, on.output,
        "{name}: handler-table output must match the match-dispatch output"
    );
    assert_eq!(
        off.output, expected_output,
        "{name}: output must match upstream Julia"
    );
    let (hits, fallbacks) = on
        .table_metrics
        .unwrap_or_else(|| panic!("{name}: gate on must arm the handler table"));
    assert!(
        hits > 0,
        "{name}: gate on must serve dispatches from hot table rows (fallbacks: {fallbacks})"
    );
    eprintln!("[handler-table parity] {name}: table_hits={hits} fallback_dispatches={fallbacks}");
}

#[test]
fn handler_table_parity_fib_recursion_issue_8562() {
    assert_gate_parity("fib", FIB_SRC, "6765\n");
}

#[test]
fn handler_table_parity_calc_pi_while_loop_issue_8562() {
    assert_gate_parity("calc_pi", CALC_PI_SRC, "3.1415826535897198\n");
}

#[test]
fn handler_table_parity_calc_pi_call_loop_issue_8562() {
    assert_gate_parity("calc_pi_call", CALC_PI_CALL_SRC, "3.1415826535897198\n");
}

#[test]
fn handler_table_parity_lorenz_accum_loop_issue_8562() {
    assert_gate_parity("lorenz_accum", LORENZ_SRC, "-139963.64116703314\n");
}
