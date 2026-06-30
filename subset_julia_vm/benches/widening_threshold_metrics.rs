//! Widening threshold metrics and sweep harness (Issue #5096).
//!
//! Run with:
//!   cargo bench --features profiling --bench widening_threshold_metrics

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::BTreeSet;
use std::hint::black_box;
use std::sync::Once;
use subset_julia_vm::compile::infer_metrics;
use subset_julia_vm::compile::lattice::{limit_type_size, ConcreteType, LatticeType};
use subset_julia_vm::inference_core::{CorePrimitive, CoreType};

const LENGTHS: &[usize] = &[4, 6, 8, 12];
const COMPLEXITIES: &[usize] = &[3, 4, 5, 6];

static REPORT_ONCE: Once = Once::new();

fn numeric_union() -> LatticeType {
    let types = [
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
    ];
    LatticeType::Union(types.into_iter().collect())
}

fn deep_array(depth: usize) -> ConcreteType {
    (0..depth).fold(
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        |element, _| ConcreteType::Array {
            element: Box::new(element),
            ndims: None,
        },
    )
}

fn recursive_growth_pair() -> (LatticeType, LatticeType) {
    (
        LatticeType::Concrete(deep_array(4)),
        LatticeType::Concrete(deep_array(3)),
    )
}

fn retained_members(ty: &LatticeType) -> usize {
    match ty {
        LatticeType::Bottom => 0,
        LatticeType::Concrete(_) => 1,
        LatticeType::Union(types) => types.len(),
        LatticeType::Const(_) | LatticeType::Conditional { .. } | LatticeType::Top => 1,
    }
}

fn run_sweep_case(length: usize, complexity: usize) -> infer_metrics::InferenceMetrics {
    let union = numeric_union();
    let (grown, compare_to) = recursive_growth_pair();
    let mut mixed = BTreeSet::new();
    mixed.insert(deep_array(1));
    mixed.insert(deep_array(2));
    mixed.insert(deep_array(3));
    mixed.insert(deep_array(4));

    infer_metrics::clear();
    let _ = black_box(limit_type_size(&union, None, length, complexity));
    let _ = black_box(limit_type_size(
        &grown,
        Some(&compare_to),
        length,
        complexity,
    ));
    let _ = black_box(limit_type_size(
        &LatticeType::Union(mixed),
        Some(&compare_to),
        length,
        complexity,
    ));
    let metrics = infer_metrics::snapshot();
    infer_metrics::clear();
    metrics
}

fn print_sweep_report() {
    REPORT_ONCE.call_once(|| {
        eprintln!("\n=== Widening Threshold Metrics (Issue #5096) ===");
        eprintln!(
            "{:>4} {:>4} {:>7} {:>7} {:>7} {:>7}",
            "len", "cx", "calls", "len_w", "cx_w", "wrap_w"
        );
        for &length in LENGTHS {
            for &complexity in COMPLEXITIES {
                let metrics = run_sweep_case(length, complexity);
                let union_after = limit_type_size(&numeric_union(), None, length, complexity);
                eprintln!(
                    "{:>4} {:>4} {:>7} {:>7} {:>7} {:>7}  retained={}",
                    length,
                    complexity,
                    metrics.limit_type_size_calls,
                    metrics.union_length_widenings,
                    metrics.union_complexity_widenings,
                    metrics.comparison_wrapper_widenings,
                    retained_members(&union_after)
                );
            }
        }
        eprintln!("================================================\n");
    });
}

fn bench_widening_threshold_metrics(c: &mut Criterion) {
    print_sweep_report();

    let mut group = c.benchmark_group("widening_threshold_metrics");
    for &length in LENGTHS {
        for &complexity in COMPLEXITIES {
            group.bench_with_input(
                BenchmarkId::new("limit_type_size", format!("len{length}_cx{complexity}")),
                &(length, complexity),
                |b, &(length, complexity)| {
                    b.iter(|| run_sweep_case(black_box(length), black_box(complexity)));
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_widening_threshold_metrics);
criterion_main!(benches);
