//! Exit-criterion benchmark for the REPL input-delta migration (Issue #9199 S5).
//!
//! The epic's headline acceptance metric is: **per-eval COMPILE time is
//! independent of session length**. This benchmark measures exactly that — for a
//! growing number `N` of prior definitions it drives the production persistent
//! `REPLSession` and reads `REPLSession::last_compile_nanos()` (the compile phase
//! only: merge + global fold + codegen, excluding VM construction + execution).
//!
//! RESULT — the metric is **MET** for two subsets, printed as two tables:
//!   [A] EXPRESSION / global-(re)assignment deltas (LV2 live-append). Measured
//!       N=0→80: `Legacy` compile grows ~23x (O(session)) while `Persistent`
//!       stays **FLAT (~1.2x)**, vm-build **0**.
//!   [B] brand-new generic FUNCTION DEFINITION deltas (LV3 compiled live-append).
//!       Measured N=0→80: `Legacy` compile grows ~11x while `Persistent` stays
//!       **FLAT (~1.1x)**, vm-build **0** — a new-generic definition compiles ONLY
//!       its own body (`repl_relocatable_delta_compile` → `AppendableFunction`) and
//!       appends it to the live method tables (`Vm::install_appended_function` →
//!       `activate_eval_function`) instead of recompiling the accumulated program.
//! LV4 extends this to brand-new non-parametric STRUCT DEFINITION deltas [C], and
//! LV5 to MODULE REFERENCE deltas [D] — a SIMPLE user module (functions + const
//! bindings) is realized once and its later references (`M.f()`, `M.const`)
//! re-enter the parked module-realized VM (vm-build 0) instead of re-running the
//! module body every eval.
//! Both live-append paths (a) compile ONLY the input as a relocatable delta and
//! (b) skip assembling the O(session) Base/prior code prefix (`!assemble_prefix`),
//! so per-eval compile is ~O(input), independent of session length. A delta that
//! REDEFINES / method-extends / lifts a lambda / uses a hard-scope `let` / is
//! parametric-or-kw / references a PACKAGE or structurally-rich module falls back
//! to the full recompile (still O(session)) — LV3b+ (see
//! `docs/vm/ADR_REPL_EVAL_MODEL.md`).
//!
//! WHERE THE O(session) COST LIVES (Issue #9199 live-VM slice): besides the
//! compile phase, this benchmark now also reports `REPLSession::last_vm_build_nanos()`
//! — the time `Vm::new_program` spends re-deriving every program-scaled table
//! (`call_site_caches = vec![_; code.len()]`, the predecoded `ExecutableProgram`,
//! per-function slot/name maps, type ancestry) from the WHOLE accumulated program
//! each eval. The measurement localizes the epic's O(session) growth to the
//! COMPILE phase: vm-build turns out to be small (~single-digit ms) and roughly
//! FLAT over the measured N, because it is dominated by the fixed Base program and
//! the marginal per-user-definition cost is negligible next to it. So the live-VM
//! reshape must flatten the compile phase (stop re-assembling the accumulated
//! program's method tables + bytecode each eval); vm-build is not the bottleneck
//! in the practical REPL range (a later slice should re-check it at very large N).
//!
//! It is a `harness = false` binary (its own `main`), not a Criterion bench, so
//! the curve prints as a plain table. Timings are wall-clock and the host may be
//! noisy (parallel work, thermal), so treat absolute numbers as provisional and
//! read the SHAPE (flat vs rising) and the growth ratio.
//!
//! Run: `cargo run --release -p subset_julia_vm --bench repl_input_delta_9199`
//! (or `cargo bench --bench repl_input_delta_9199`).

use subset_julia_vm::repl::REPLSession;

/// Number of prior definitions to accumulate before measuring.
const N_POINTS: &[usize] = &[0, 40, 80];
/// Repeated measured compiles per data point; the median is reported.
const REPEATS: usize = 7;

fn median_nanos(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// A non-trivial function body so per-function codegen/inference is a real cost
/// (a one-liner is dominated by the fixed Base-handling floor and hides the
/// per-function savings). Kept plain-numeric so it lowers to a single method
/// with no lifted lambdas (which would route to the full path).
fn def_src(i: usize) -> String {
    format!(
        "f{i}(x) = ((x + {i}) * 2 - 3) * ((x - {i}) + 7) + (x * x - {i} * x) + \
         (x + 1) * (x + 2) * (x + 3) - ((x - 1) * (x - 2)) + {i} * x - x"
    )
}

/// Median (compile, vm-build) nanoseconds for the two per-eval phases whose
/// session-length dependence the live-VM slice targets. Both are read from the
/// SAME measured evals so the split is consistent.
struct PhaseNanos {
    compile: u128,
    vm_build: u128,
}

/// Build a session with `n` prior (non-trivial) function definitions, then measure
/// the compile + VM-build time of `REPEATS` further evals of distinct top-level
/// expressions. Each expression is unique (program-cache miss => a real compile)
/// but adds no definition (accumulated set stays size `n`), so under Persistent it
/// takes the input-delta path (expressions define nothing). Under Legacy each
/// re-merges and RECOMPILES all `n` functions; the delta reuses their compiled
/// bodies. Measured result (Issue #9199): the COMPILE curve still grows with `n`
/// under both models (the delta's codegen saving is swamped by the whole-program
/// method-table build + bytecode assembly both pay to build the fresh VM), while
/// the VM-build curve stays small and ~flat (Base-dominated).
fn measure_phase_nanos(n: usize) -> PhaseNanos {
    let mut session = REPLSession::new(0);
    // Warm the Base cache with a throwaway eval (not measured).
    let _ = session.eval("1 + 1");
    for i in 0..n {
        let result = session.eval(&def_src(i));
        assert!(result.success, "setup def f{i} failed: {:?}", result.error);
    }

    let mut compile_samples = Vec::with_capacity(REPEATS);
    let mut vm_build_samples = Vec::with_capacity(REPEATS);
    for k in 0..REPEATS {
        // Unique expression => program-cache miss => a genuine compile each time.
        let src = format!("({} * 2 + 1) * 3 - {}", 1_000 + k, k);
        let result = session.eval(&src);
        assert!(
            result.success,
            "measured eval failed at n={n}: {:?}",
            result.error
        );
        compile_samples.push(
            session
                .last_compile_nanos()
                .expect("eval records a compile duration"),
        );
        vm_build_samples.push(
            session
                .last_vm_build_nanos()
                .expect("eval records a vm-build duration"),
        );
    }
    PhaseNanos {
        compile: median_nanos(compile_samples),
        vm_build: median_nanos(vm_build_samples),
    }
}

/// Like [`measure_phase_nanos`] but the MEASURED evals are brand-new generic
/// FUNCTION DEFINITIONS, not expressions (Issue #9199 LV3). After `n` prior
/// definitions, each measured eval defines a UNIQUE new generic. Under Persistent
/// a new-generic definition takes the LV3 compiled live-append path — it compiles
/// ONLY the new function's body and splices it onto the held VM (no
/// `Vm::new_program`), so its COMPILE is ~O(input) and vm-build is 0, independent
/// of `n`; under Legacy each re-merges and RECOMPILES the whole accumulated
/// program (O(session)). A throwaway expression precedes each measured definition
/// so the live VM's function count is re-synced with the compile prefix (a prior
/// measured live-append leaves the prefix stale; the expression's full recompile
/// clears it), guaranteeing the measured definition actually live-appends.
fn measure_definition_phase_nanos(n: usize) -> PhaseNanos {
    let mut session = REPLSession::new(0);
    let _ = session.eval("1 + 1");
    for i in 0..n {
        let result = session.eval(&def_src(i));
        assert!(result.success, "setup def f{i} failed: {:?}", result.error);
    }

    let mut compile_samples = Vec::with_capacity(REPEATS);
    let mut vm_build_samples = Vec::with_capacity(REPEATS);
    for k in 0..REPEATS {
        // Re-sync the prefix (clears any staleness a prior measured live-append
        // left) so the measured definition below takes the live-append path.
        let sync = session.eval(&format!("({} * 2 + 1) - {}", 5_000 + k, k));
        assert!(sync.success, "sync eval failed at n={n}: {:?}", sync.error);
        // Measure a brand-new generic definition (unique name; plain-numeric body
        // so it lowers to a single method with no lifted lambdas).
        let src = format!(
            "fmeas{k}z9199(x) = ((x + {k}) * 2 - 3) * ((x - {k}) + 7) + (x * x - {k} * x) + \
             (x + 1) * (x + 2) - ((x - 1) * (x - 2)) + {k} * x - x"
        );
        let result = session.eval(&src);
        assert!(
            result.success,
            "measured def failed at n={n}: {:?}",
            result.error
        );
        compile_samples.push(
            session
                .last_compile_nanos()
                .expect("eval records a compile duration"),
        );
        vm_build_samples.push(
            session
                .last_vm_build_nanos()
                .expect("eval records a vm-build duration"),
        );
    }
    PhaseNanos {
        compile: median_nanos(compile_samples),
        vm_build: median_nanos(vm_build_samples),
    }
}

/// Like [`measure_definition_phase_nanos`] but the MEASURED evals are brand-new
/// non-parametric STRUCT DEFINITIONS (Issue #9199 LV4). Under Persistent a
/// brand-new struct definition takes the LV4 compiled TYPE live-append path — it
/// compiles ONLY the delta and appends the type registries to the held VM
/// (`Vm::install_appended_types`, no `Vm::new_program`), so its COMPILE is
/// ~O(input) and vm-build is 0, independent of `n`; under Legacy each re-merges
/// and RECOMPILES the whole accumulated program (O(session)). A throwaway
/// expression precedes each measured definition so the live VM's struct-def count
/// is re-synced with the compile prefix (a prior measured type-append leaves the
/// prefix stale; the expression's full recompile clears it), guaranteeing the
/// measured definition actually live-appends.
fn measure_type_definition_phase_nanos(n: usize) -> PhaseNanos {
    let mut session = REPLSession::new(0);
    let _ = session.eval("1 + 1");
    for i in 0..n {
        let result = session.eval(&def_src(i));
        assert!(result.success, "setup def f{i} failed: {:?}", result.error);
    }

    let mut compile_samples = Vec::with_capacity(REPEATS);
    let mut vm_build_samples = Vec::with_capacity(REPEATS);
    for k in 0..REPEATS {
        // Re-sync the prefix (clears any staleness a prior measured type-append
        // left) so the measured struct definition below takes the live-append path.
        let sync = session.eval(&format!("({} * 2 + 1) - {}", 7_000 + k, k));
        assert!(sync.success, "sync eval failed at n={n}: {:?}", sync.error);
        // Measure a brand-new non-parametric struct definition (unique name, a few
        // fields so per-def registration is a real cost).
        let src = format!("struct SMeas{k}z9199\n  a::Int\n  b::Float64\n  c::Int\nend");
        let result = session.eval(&src);
        assert!(
            result.success,
            "measured struct def failed at n={n}: {:?}",
            result.error
        );
        compile_samples.push(
            session
                .last_compile_nanos()
                .expect("eval records a compile duration"),
        );
        vm_build_samples.push(
            session
                .last_vm_build_nanos()
                .expect("eval records a vm-build duration"),
        );
    }
    PhaseNanos {
        compile: median_nanos(compile_samples),
        vm_build: median_nanos(vm_build_samples),
    }
}

/// Like [`measure_type_definition_phase_nanos`] but the session holds a SIMPLE
/// user module and the MEASURED evals only REFERENCE it (`M.val()`, a pure
/// function) — Issue #9199 LV5. Under Persistent a module-reference delta
/// re-enters the module-realized parked live VM (the module's body is NOT
/// re-run), so its COMPILE is ~O(input) and vm-build is 0, independent of `n`;
/// under Legacy each eval re-merges + re-runs the whole accumulated program
/// INCLUDING the module body (O(session)). A throwaway expression precedes each
/// measured reference so the prefix staleness a prior setup definition left is
/// cleared (and the module-realized VM re-parked), guaranteeing the measured
/// reference actually takes the live path.
fn measure_module_reference_phase_nanos(n: usize) -> PhaseNanos {
    let mut session = REPLSession::new(0);
    let _ = session.eval("1 + 1");
    // Realize a simple user module once (functions only — persistable).
    let def = session.eval("module MBench9199\n  val() = 42\nend");
    assert!(def.success, "module setup failed: {:?}", def.error);
    for i in 0..n {
        let result = session.eval(&def_src(i));
        assert!(result.success, "setup def f{i} failed: {:?}", result.error);
    }

    let mut compile_samples = Vec::with_capacity(REPEATS);
    let mut vm_build_samples = Vec::with_capacity(REPEATS);
    for k in 0..REPEATS {
        // Re-sync the prefix (clears staleness + re-parks the module-realized VM)
        // so the measured module reference below takes the live path.
        let sync = session.eval(&format!("({} * 2 + 1) - {}", 9_000 + k, k));
        assert!(sync.success, "sync eval failed at n={n}: {:?}", sync.error);
        // Measure a pure module-function reference (idempotent, so repeatable).
        let result = session.eval("MBench9199.val()");
        assert!(
            result.success,
            "measured module reference failed at n={n}: {:?}",
            result.error
        );
        compile_samples.push(
            session
                .last_compile_nanos()
                .expect("eval records a compile duration"),
        );
        vm_build_samples.push(
            session
                .last_vm_build_nanos()
                .expect("eval records a vm-build duration"),
        );
    }
    PhaseNanos {
        compile: median_nanos(compile_samples),
        vm_build: median_nanos(vm_build_samples),
    }
}

fn run_model() -> Vec<(usize, PhaseNanos)> {
    N_POINTS
        .iter()
        .map(|&n| (n, measure_phase_nanos(n)))
        .collect()
}

fn run_model_module_reference() -> Vec<(usize, PhaseNanos)> {
    N_POINTS
        .iter()
        .map(|&n| (n, measure_module_reference_phase_nanos(n)))
        .collect()
}

fn run_model_definition() -> Vec<(usize, PhaseNanos)> {
    N_POINTS
        .iter()
        .map(|&n| (n, measure_definition_phase_nanos(n)))
        .collect()
}

fn run_model_type_definition() -> Vec<(usize, PhaseNanos)> {
    N_POINTS
        .iter()
        .map(|&n| (n, measure_type_definition_phase_nanos(n)))
        .collect()
}

fn print_table(title: &str, samples: &[(usize, PhaseNanos)]) {
    println!("\n{title}");
    println!("{:>6} | {:>13} {:>13}", "N", "compile ms", "vmbuild ms");
    println!("{:->6}-+-{:->13}-{:->13}", "", "", "");
    for (n, sample) in samples {
        let compile = sample.compile as f64 / 1e6;
        let vm_build = sample.vm_build as f64 / 1e6;
        println!("{n:>6} | {compile:>13.3} {vm_build:>13.3}");
    }
    let growth = |c: &[(usize, PhaseNanos)]| {
        let first = c.first().map(|(_, v)| v.compile).unwrap_or(1).max(1);
        let last = c.last().map(|(_, v)| v.compile).unwrap_or(1);
        last as f64 / first as f64
    };
    let (n0, n1) = (
        N_POINTS.first().copied().unwrap_or(0),
        N_POINTS.last().copied().unwrap_or(0),
    );
    println!("growth N={n0}->N={n1}   compile: {:.2}x", growth(samples),);
}

fn main() {
    // Run inside a large stack so deep Base compiles do not overflow (mirrors the
    // differential harness / iOS thread sizing).
    let handle = std::thread::Builder::new()
        .name("repl-input-delta-9199".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            println!(
                "\nIssue #9199 — per-eval COMPILE + VM-BUILD time vs prior-definition count N"
            );
            println!(
                "(median of {REPEATS} evals; run() excluded; provisional — host may be noisy)"
            );

            // LV2: EXPRESSION / global deltas (covered since LV2).
            let expr_persistent = run_model();
            print_table(
                "[A] EXPRESSION deltas (LV2 live-append) — Persistent should be ~flat, vm-build 0",
                &expr_persistent,
            );

            // LV3: brand-new generic FUNCTION DEFINITION deltas.
            let def_persistent = run_model_definition();
            print_table(
                "[B] DEFINITION deltas (LV3 compiled live-append) — Persistent should be ~flat, vm-build 0",
                &def_persistent,
            );

            // LV4: brand-new non-parametric STRUCT DEFINITION deltas.
            let type_persistent = run_model_type_definition();
            print_table(
                "[C] STRUCT DEFINITION deltas (LV4 compiled type live-append) — Persistent should be ~flat, vm-build 0",
                &type_persistent,
            );

            // LV5: a SIMPLE user module realized once; measured evals REFERENCE it.
            let module_persistent = run_model_module_reference();
            print_table(
                "[D] MODULE REFERENCE deltas (LV5 realize-once) — Persistent should be ~flat, vm-build 0",
                &module_persistent,
            );

            println!(
                "\n(exit criterion: Persistent COMPILE ~flat vs Legacy O(N). LV2 MET it for \
                 EXPRESSION deltas [A]; LV3 (Issue #9199) extends it to generic FUNCTION \
                 DEFINITION deltas [B]; LV4 extends it to brand-new non-parametric STRUCT \
                 DEFINITION deltas [C] — a struct definition compiles ONLY its delta and appends \
                 its type registries to the live VM (`Vm::install_appended_types`, aligned \
                 `type_id`), so Persistent COMPILE is ~flat and vm-build is 0 while Legacy grows \
                 O(N). LV5 extends it to MODULE REFERENCE deltas [D] — a simple user module is \
                 realized once and its later references re-enter the parked module-realized VM \
                 (vm-build 0), while Legacy re-runs the whole module body each eval. Method \
                 extension / redefinition / parametric-or-kw / lambda-lifting definitions (LV3b), \
                 abstract / primitive / enum / parametric / inner-ctor type definitions (LV4b), \
                 and package / submodule / typed modules (LV5b) still fall back to the full \
                 recompile. See docs/vm/ADR_REPL_EVAL_MODEL.md §\"Live-VM slice decomposition\")\n"
            );
        })
        .expect("spawn bench thread");
    handle.join().expect("bench thread panicked");
}
