//! Issue #8168: per-call-site cache for the `CallDynamicBinaryBoth` resolver.
//!
//! When two `Any`-typed struct operands flow into a binary operator (here `+`
//! on a `Vector{Any}` of `V2`), the operator compiles to
//! `CallDynamicBinaryBoth` and the VM must pick the matching method from the
//! operator's full candidate list on every call. The dispatch decision is fully
//! determined by the operand type names for struct/struct pairs, so it is
//! memoized per call site. These tests pin both the correctness of the cached
//! decision and that the fast path is actually taken.

#[cfg(feature = "profiling")]
use std::collections::HashMap;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
#[cfg(feature = "profiling")]
use subset_julia_vm::vm::profiler;
use subset_julia_vm::vm::{CompiledProgram, Value, Vm};

const ANY_STRUCT_ADD_SOURCE: &str = r#"
struct V2
    x::Float64
    y::Float64
end
import Base: +
+(a::V2, b::V2) = V2(a.x + b.x, a.y + b.y)

function sumany(xs, n)
    acc = xs[1]
    for _ in 1:n
        for k in 1:length(xs)
            acc = acc + xs[k]
        end
    end
    acc.x + acc.y
end

xs = Any[V2(1.0, 2.0), V2(3.0, 4.0)]
sumany(xs, 3)
"#;

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    compile_with_cache(&program).expect("compile source")
}

fn run_any_struct_add() -> Value {
    let mut vm = Vm::new_program(compile_source(ANY_STRUCT_ADD_SOURCE), StableRng::new(0));
    vm.run().expect("run sumany")
}

/// The cached dynamic dispatch must select the user `+(::V2, ::V2)` method on
/// every iteration, matching upstream Julia's result (33.0). Guards against the
/// cache returning a stale / wrong method index. (Issue #8168)
#[test]
fn any_struct_add_cached_dispatch_matches_upstream_8168() {
    match run_any_struct_add() {
        Value::F64(value) => assert!(
            (value - 33.0).abs() < 1.0e-12,
            "unexpected sumany result: {value}"
        ),
        other => panic!("expected Float64 result, got {other:?}"),
    }
}

/// The repeated struct/struct dispatch must take the per-call-site resolver
/// cache after the first resolution. (Issue #8168)
#[cfg(feature = "profiling")]
#[test]
fn any_struct_add_takes_binary_both_resolver_cache_8168() {
    profiler::clear();
    profiler::enable();
    let result = run_any_struct_add();
    profiler::disable();

    match result {
        Value::F64(value) => assert!((value - 33.0).abs() < 1.0e-12),
        other => panic!("expected Float64 result, got {other:?}"),
    }

    let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
    assert!(
        counts
            .get("BinaryBothResolverCacheHit")
            .copied()
            .unwrap_or(0)
            > 0,
        "repeated struct/struct + should hit the binary-both resolver cache: {counts:?}"
    );
}
