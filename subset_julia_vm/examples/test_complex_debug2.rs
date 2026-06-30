use subset_julia_vm::compile_and_run_str;

fn main() {
    // Test A: Simple z^2
    let test_a = r#"
function test_a(c::Complex)
    z = 0.0 + 0.0im
    z = z^2
    return 1
end
test_a(1.0 + 0.0im)
"#;
    println!("Test A: z = z^2");
    let result = compile_and_run_str(test_a, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test B: z^2 + c
    let test_b = r#"
function test_b(c::Complex)
    z = 0.0 + 0.0im
    z = z^2 + c
    return 1
end
test_b(1.0 + 0.0im)
"#;
    println!("\nTest B: z = z^2 + c");
    let result = compile_and_run_str(test_b, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test C: for loop with z^2
    let test_c = r#"
function test_c(c::Complex)
    z = 0.0 + 0.0im
    for k in 1:5
        z = z^2
    end
    return 1
end
test_c(1.0 + 0.0im)
"#;
    println!("\nTest C: for loop with z^2");
    let result = compile_and_run_str(test_c, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test D: for loop with z^2 + c
    let test_d = r#"
function test_d(c::Complex)
    z = 0.0 + 0.0im
    for k in 1:5
        z = z^2 + c
    end
    return 1
end
test_d(1.0 + 0.0im)
"#;
    println!("\nTest D: for loop with z^2 + c");
    let result = compile_and_run_str(test_d, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test E: for loop with abs2 check only
    let test_e = r#"
function test_e(c::Complex)
    z = 0.0 + 0.0im
    for k in 1:5
        if abs2(z) > 4.0
            return k
        end
    end
    return 5
end
test_e(1.0 + 0.0im)
"#;
    println!("\nTest E: for loop with abs2 check only");
    let result = compile_and_run_str(test_e, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test F: for loop with abs2 check AND z^2
    let test_f = r#"
function test_f(c::Complex)
    z = 0.0 + 0.0im
    for k in 1:5
        if abs2(z) > 4.0
            return k
        end
        z = z^2
    end
    return 5
end
test_f(1.0 + 0.0im)
"#;
    println!("\nTest F: for loop with abs2 AND z^2");
    let result = compile_and_run_str(test_f, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test G: for loop with abs2 check AND z^2 + c
    let test_g = r#"
function test_g(c::Complex)
    z = 0.0 + 0.0im
    for k in 1:5
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return 5
end
test_g(1.0 + 0.0im)
"#;
    println!("\nTest G: for loop with abs2 AND z^2 + c");
    let result = compile_and_run_str(test_g, 0);
    println!("  Result: {} (NaN means error)", result);
}
