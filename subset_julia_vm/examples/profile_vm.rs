//! VM instruction profiler example
//!
//! Profiles instruction frequency during execution to identify optimization opportunities.
//!
//! Run with: cargo run --example profile_vm

use subset_julia_vm::{compile_and_run_str, vm::profiler};

fn main() {
    // Enable profiling
    profiler::enable();
    profiler::clear();

    println!("Profiling calc_pi(100)...\n");

    let source = r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

calc_pi(100)
"#;

    let result = compile_and_run_str(source, 0);
    println!("Result: π ≈ {:.6}\n", result);

    // Print profiling results
    profiler::print_results();

    profiler::disable();
}
