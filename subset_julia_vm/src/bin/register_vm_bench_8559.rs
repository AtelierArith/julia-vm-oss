//! Cross-target register VM vs stack VM measurement harness (Issue #8559).
//!
//! Runs the Issue #8448 benchmark set — `fib(25)`-class recursion,
//! `calc_pi(1_000_000)`-class loop, and an attractor-style `Float64` loop —
//! once per engine (register VM gate off = production stack VM, gate on =
//! eligible bodies on the register VM prototype) and prints:
//!
//! - static metrics: stack vs register bytecode size, register/slot counts,
//!   estimated per-frame memory
//! - deterministic dynamic counters (load-independent): stack VM dispatches,
//!   executable-block runs, operand-stack / call-frame high-water marks,
//!   register VM calls / fallbacks / dispatches
//! - wall times over `reps` fresh-`Vm` runs of the precompiled program
//!   (VM execution only; parsing/lowering/compilation excluded), with metrics
//!   collection disabled so counters do not perturb timing
//!
//! The same binary runs on the macOS host and on the iOS Simulator
//! (`cargo build --release -p subset_julia_vm --bin register_vm_bench_8559
//! --target aarch64-apple-ios-sim`, then `xcrun simctl spawn <device> <bin>`).
//! The Wasm counterpart is `scripts/register_vm_wasm_bench_8559.mjs` driving
//! `subset_julia_vm_web::run_register_vm_bench`.
//!
//! Usage: `register_vm_bench_8559 [reps]` (default 5). Follow the repository
//! machine-quiet benchmark protocol for the wall-time columns; the counter
//! columns are deterministic and unaffected by load.

use std::time::Instant;

use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
use subset_julia_vm::register_vm::{RegisterInstr, RegisterProgram};
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{
    set_register_vm_forced, set_stack_vm_metrics_forced, StackVmMetrics, Vm,
};
use subset_julia_vm_bytecode::{CompiledProgram, Instr, Value};

/// Recursive benchmark (`fib`-class, Issue #8448 target list). Upstream
/// Julia: 75025.
const FIB_SRC: &str = r#"
function fib(n::Int64)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

println(fib(25))
"#;

/// Loop benchmark (`calc_pi`-class). Upstream Julia: 3.1415916535897743.
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

println(calc_pi(1000000))
"#;

/// Attractor-style Float64 loop (Lorenz step + accumulator; slot-heavy F64
/// arithmetic). Upstream Julia: -11779.830551874697.
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

println(lorenz_accum(1000000))
"#;

struct Bench {
    name: &'static str,
    src: &'static str,
    /// Function whose body feeds the static (bytecode-size) columns.
    func_name: &'static str,
    expected_output: &'static str,
}

const BENCHES: [Bench; 3] = [
    Bench {
        name: "fib(25)",
        src: FIB_SRC,
        func_name: "fib",
        expected_output: "75025\n",
    },
    Bench {
        name: "calc_pi(1_000_000)",
        src: CALC_PI_SRC,
        func_name: "calc_pi",
        expected_output: "3.1415916535897743\n",
    },
    Bench {
        name: "lorenz_accum(1_000_000)",
        src: LORENZ_SRC,
        func_name: "lorenz_accum",
        expected_output: "-11779.830551874697\n",
    },
];

fn compile(src: &str) -> CompiledProgram {
    let program = parse_and_lower_with_base_dir(src, None)
        .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
    compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"))
}

struct CounterRun {
    output: String,
    stack: StackVmMetrics,
    register_calls: u64,
    register_fallbacks: u64,
    register_dispatches: u64,
}

/// One instrumented run: deterministic counters for both engines.
fn counter_run(compiled: &CompiledProgram) -> CounterRun {
    set_stack_vm_metrics_forced(true);
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    set_stack_vm_metrics_forced(false);
    vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
    CounterRun {
        output: vm.get_output().to_string(),
        stack: vm.stack_vm_metrics().unwrap_or_else(|| {
            eprintln!("internal error: stack VM metrics were forced on but not collected");
            std::process::exit(1);
        }),
        register_calls: vm.register_vm_executed_calls(),
        register_fallbacks: vm.register_vm_fallback_calls(),
        register_dispatches: vm.register_vm_dispatch_total(),
    }
}

/// `reps` uninstrumented wall-time runs of the precompiled program
/// (fresh `Vm` per run, timing `Vm::run` only). Returns times in ms.
fn wall_runs(compiled: &CompiledProgram, reps: usize) -> Vec<f64> {
    (0..reps)
        .map(|_| {
            let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
            let start = Instant::now();
            vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect()
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    let mid = samples.len() / 2;
    if samples.len().is_multiple_of(2) {
        (samples[mid - 1] + samples[mid]) / 2.0
    } else {
        samples[mid]
    }
}

fn main() {
    let reps: usize = std::env::args()
        .nth(1)
        .map(|arg| {
            arg.parse().unwrap_or_else(|e| {
                eprintln!("invalid reps '{arg}': {e}");
                std::process::exit(2);
            })
        })
        .unwrap_or(5);
    if reps < 1 {
        eprintln!("invalid reps '{reps}': must be >= 1");
        std::process::exit(2);
    }

    println!(
        "# register_vm_bench_8559 (target: {}, profile: {}, reps: {reps})",
        target_description(),
        profile_description(),
    );
    println!(
        "# sizes: Instr={}B RegisterInstr={}B Value={}B stack-frame-struct={}B",
        std::mem::size_of::<Instr>(),
        std::mem::size_of::<RegisterInstr>(),
        std::mem::size_of::<Value>(),
        subset_julia_vm::vm::stack_metrics::frame_struct_size_bytes(),
    );

    let mut parity_failed = false;
    for bench in &BENCHES {
        let compiled = compile(bench.src);

        // ---- static per-function metrics ----
        let func = compiled
            .functions
            .iter()
            .find(|f| f.name == bench.func_name)
            .unwrap_or_else(|| panic!("{}: function {} not found", bench.name, bench.func_name));
        let stack_instrs = func.code_end - func.entry;
        let stack_bytes = stack_instrs * std::mem::size_of::<Instr>();
        let register_program = RegisterProgram::from_stack_function(&compiled.code, func)
            .unwrap_or_else(|e| panic!("{}: body must translate: {e}", bench.name));
        let metrics = register_program.metrics();
        let value_slot = std::mem::size_of::<Option<Value>>();
        let register_frame_bytes = (metrics.frame_registers + metrics.frame_slots) * value_slot;

        println!("\n## {}", bench.name);
        println!(
            "static: stack_instrs={stack_instrs} stack_bytes={stack_bytes} \
             register_instrs={} register_bytes={} frame_registers={} frame_slots={} \
             register_frame_bytes={register_frame_bytes}",
            metrics.dispatch_count,
            metrics.bytecode_bytes,
            metrics.frame_registers,
            metrics.frame_slots,
        );

        // ---- deterministic counters, both engines ----
        set_register_vm_forced(false);
        let off = counter_run(&compiled);
        set_register_vm_forced(true);
        let on = counter_run(&compiled);
        set_register_vm_forced(false);

        if off.output != bench.expected_output || on.output != bench.expected_output {
            parity_failed = true;
            println!(
                "PARITY FAIL: expected {:?}, stack {:?}, register {:?}",
                bench.expected_output, off.output, on.output
            );
        }
        println!(
            "counters[stack-vm  ]: dispatches={} executable_blocks={} \
             operand_stack_high_water={} frames_high_water={}",
            off.stack.dispatches,
            off.stack.executable_block_runs,
            off.stack.operand_stack_high_water,
            off.stack.frames_high_water,
        );
        println!(
            "counters[register  ]: register_calls={} register_fallbacks={} \
             register_dispatches={}",
            on.register_calls, on.register_fallbacks, on.register_dispatches,
        );
        println!(
            "counters[reg-resid ]: stack_dispatches={} executable_blocks={} \
             operand_stack_high_water={} frames_high_water={}",
            on.stack.dispatches,
            on.stack.executable_block_runs,
            on.stack.operand_stack_high_water,
            on.stack.frames_high_water,
        );

        // ---- wall times (uninstrumented) ----
        set_register_vm_forced(false);
        let mut off_ms = wall_runs(&compiled, reps);
        set_register_vm_forced(true);
        let mut on_ms = wall_runs(&compiled, reps);
        set_register_vm_forced(false);
        println!(
            "wall_ms: stack median={:.3} min={:.3} | register median={:.3} min={:.3} (samples: stack {:?} register {:?})",
            median(&mut off_ms),
            off_ms.first().copied().unwrap_or(f64::NAN),
            median(&mut on_ms),
            on_ms.first().copied().unwrap_or(f64::NAN),
            off_ms.iter().map(|ms| (ms * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
            on_ms.iter().map(|ms| (ms * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
        );
    }

    if parity_failed {
        println!("\nPARITY FAILURES DETECTED");
        std::process::exit(1);
    }
    println!("\nall benchmarks matched the upstream-Julia-pinned output on both engines");
}

fn target_description() -> &'static str {
    if cfg!(target_os = "ios") {
        if cfg!(target_abi = "sim") {
            "aarch64-apple-ios-sim"
        } else {
            "ios"
        }
    } else if cfg!(target_arch = "wasm32") {
        "wasm32"
    } else if cfg!(target_os = "macos") {
        "macos-host"
    } else {
        "other"
    }
}

fn profile_description() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}
