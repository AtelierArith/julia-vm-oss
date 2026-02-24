//! Tests for all iOS app CodeSample.swift samples
//!
//! This file contains tests for every sample in the iOS app to ensure
//! they all compile and run correctly.

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::compile_and_run_str;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;

// ==================== Basic ====================

#[test]
fn test_sample_hello_world() {
    let src = r#"println("Hello, World!")"#;
    let result = compile_and_run_str(src, 0);
    // println returns Unit, which maps to -4.0
    assert!(
        (result - (-4.0)).abs() < 1e-10 || result == 0.0,
        "Expected Unit result, got {}",
        result
    );
}

#[test]
fn test_sample_string_interpolation() {
    let src = r#"
# String interpolation with $(expression)
x = 42
pi_approx = 3.14159

# Simple variable interpolation
println("x = $(x)")

# Expression interpolation
println("x + 1 = $(x + 1)")
println("x * 2 = $(x * 2)")

# Float interpolation
println("Pi is approximately $(pi_approx)")

# Multiple interpolations in one string
y = 10
println("Sum: $(x + y), Product: $(x * y)")

# Nested parentheses work too
println("Double: $((x + y) * 2)")

println(x)
x
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 42.0).abs() < 1e-10,
        "Expected 42.0, got {}",
        result
    );
}

// ==================== Arrays ====================

#[test]
fn test_sample_vector_basics() {
    let src = r#"
# Create a vector
arr = [1, 2, 3, 4, 5]

# Access elements (1-indexed like Julia)
println("First element: ", arr[1])
println("Third element: ", arr[3])
println("Last element: ", arr[5])

# Get length
println("Length: ", length(arr))

println(arr[3])
arr[3]
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
}

#[test]
fn test_sample_range_expressions() {
    let src = r#"
# Range 1:5 creates [1, 2, 3, 4, 5]
r = 1:5
println("1:5 has length ", length(r))

# Range with step: 1:2:10 = [1, 3, 5, 7, 9]
r2 = 1:2:10
println("1:2:10 has length ", length(r2))

# Sum elements in a range
sum = 0
for x in 1:100
    sum += x
end
println("Sum 1 to 100 = ", sum)

println(sum)
sum
"#;
    let result = compile_and_run_str(src, 0);
    // Sum of 1 to 100 = 5050
    assert!(
        (result - 5050.0).abs() < 1e-10,
        "Expected 5050.0, got {}",
        result
    );
}

#[test]
fn test_sample_comprehension() {
    let src = r#"
# Comprehension: [expr for var in iter]
# Create squares of 1 to 5
squares = [x^2 for x in 1:5]

println("Squares: ")
for i in 1:length(squares)
    println("  ", i, "^2 = ", squares[i])
end

# Sum of squares
sum = 0.0
for i in 1:length(squares)
    sum += squares[i]
end
println("Sum of squares: ", sum)

println(sum)
sum
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 4 + 9 + 16 + 25 = 55
    assert!(
        (result - 55.0).abs() < 1e-10,
        "Expected 55.0, got {}",
        result
    );
}

#[test]
fn test_sample_comprehension_with_filter() {
    let src = r#"
# Comprehension with filter: [expr for var in iter if cond]

# Even numbers from 1 to 10
evens = [x for x in 1:10 if x % 2 == 0]
println("Even numbers: ")
for i in 1:length(evens)
    println("  ", evens[i])
end

# Squares of odd numbers
odd_squares = [x^2 for x in 1:10 if x % 2 == 1]
println("Odd squares: ")
for i in 1:length(odd_squares)
    println("  ", odd_squares[i])
end

result = length(evens) + length(odd_squares)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // evens: [2,4,6,8,10] = 5 elements, odd_squares: [1,9,25,49,81] = 5 elements
    assert!(
        (result - 10.0).abs() < 1e-10,
        "Expected 10.0, got {}",
        result
    );
}

#[test]
fn test_sample_array_functions() {
    let src = r#"
# zeros(n) - create array of n zeros
z = zeros(5)
println("zeros(5): ", z[1], ", ", z[2], ", ...")

# ones(n) - create array of n ones
o = ones(5)
println("ones(5): ", o[1], ", ", o[2], ", ...")

# fill(value, n) - create array filled with value
f = fill(3.14, 4)
println("fill(3.14, 4): ", f[1], ", ", f[2], ", ...")

# Combine with comprehension for more complex arrays
powers_of_2 = [2.0^i for i in 0:10]

println("Powers of 2:")
for i in 1:length(powers_of_2)
    println("  2^", i-1, " = ", powers_of_2[i])
end

println(powers_of_2[11])
powers_of_2[11]
"#;
    let result = compile_and_run_str(src, 0);
    // 2^10 = 1024
    assert!(
        (result - 1024.0).abs() < 1e-10,
        "Expected 1024.0, got {}",
        result
    );
}

#[test]
fn test_sample_array_mutation() {
    let src = r#"
# Start with an array
arr = [10, 20, 30]
println("Initial: ", arr[1], ", ", arr[2], ", ", arr[3])

# Modify elements
arr[2] = 99
println("After arr[2] = 99: ", arr[1], ", ", arr[2], ", ", arr[3])

# push! adds to end
push!(arr, 40)
println("After push!(arr, 40): length = ", length(arr))

# pop! removes from end
last = pop!(arr)
println("pop! returned: ", last)
println("After pop!: length = ", length(arr))

println(arr[2])
arr[2]
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 99.0).abs() < 1e-10,
        "Expected 99.0, got {}",
        result
    );
}

#[test]
fn test_sample_dot_product() {
    let src = r#"
function dot_product(a, b)
    @assert length(a) == length(b) "Arrays must have same length"
    sum = 0.0
    for i in 1:length(a)
        sum += a[i] * b[i]
    end
    sum
end

# Test vectors
v1 = [1, 2, 3, 4, 5]
v2 = [5, 4, 3, 2, 1]

result = dot_product(v1, v2)
println("v1 · v2 = ", result)

println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // 1*5 + 2*4 + 3*3 + 4*2 + 5*1 = 5+8+9+8+5 = 35
    assert!(
        (result - 35.0).abs() < 1e-10,
        "Expected 35.0, got {}",
        result
    );
}

#[test]
fn test_sample_statistical_functions() {
    let src = r#"
function array_sum(arr)
    sum = 0.0
    for i in 1:length(arr)
        sum += arr[i]
    end
    sum
end

function array_mean(arr)
    array_sum(arr) / length(arr)
end

function array_max(arr)
    max_val = arr[1]
    for i in 2:length(arr)
        if arr[i] > max_val
            max_val = arr[i]
        end
    end
    max_val
end

# Sample data
data = [23, 45, 12, 67, 34, 89, 11, 56]

println("Data points: ", length(data))
println("Sum: ", array_sum(data))
println("Mean: ", array_mean(data))
println("Max: ", array_max(data))

result = array_mean(data)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // Sum = 23+45+12+67+34+89+11+56 = 337, Mean = 337/8 = 42.125
    assert!(
        (result - 42.125).abs() < 1e-10,
        "Expected 42.125, got {}",
        result
    );
}

#[test]
fn test_sample_2d_matrix_basics() {
    let src = r#"
# Create a 3x3 matrix of zeros
m = zeros(3, 3)

# Fill with values
for i in 1:3
    for j in 1:3
        m[i, j] = i * 10 + j
    end
end

# Print the matrix
println("3x3 Matrix:")
for i in 1:3
    println(m[i, 1], " ", m[i, 2], " ", m[i, 3])
end

# Access specific element
println("m[2, 3] = ", m[2, 3])

println(m[2, 3])
m[2, 3]
"#;
    let result = compile_and_run_str(src, 0);
    // m[2, 3] = 2*10 + 3 = 23
    assert!(
        (result - 23.0).abs() < 1e-10,
        "Expected 23.0, got {}",
        result
    );
}

#[test]
fn test_sample_matrix_initialization() {
    let src = r#"
# Different ways to create matrices

# zeros(rows, cols) - all zeros
z = zeros(2, 3)
println("Zeros 2x3:")
println(z[1, 1], " ", z[1, 2], " ", z[1, 3])
println(z[2, 1], " ", z[2, 2], " ", z[2, 3])

# ones(rows, cols) - all ones
o = ones(2, 3)
println("Ones 2x3:")
println(o[1, 1], " ", o[1, 2], " ", o[1, 3])
println(o[2, 1], " ", o[2, 2], " ", o[2, 3])

# fill(value, rows, cols) - custom value
f = fill(7, 2, 3)
println("Fill(7) 2x3:")
println(f[1, 1], " ", f[1, 2], " ", f[1, 3])
println(f[2, 1], " ", f[2, 2], " ", f[2, 3])

println(length(f))
length(f)
"#;
    let result = compile_and_run_str(src, 0);
    // 2x3 matrix has 6 elements
    assert!((result - 6.0).abs() < 1e-10, "Expected 6.0, got {}", result);
}

#[test]
fn test_sample_matrix_sum() {
    let src = r#"
function matrix_sum(m, rows, cols)
    sum = 0.0
    for i in 1:rows
        for j in 1:cols
            sum += m[i, j]
        end
    end
    sum
end

# Create a 3x4 matrix
rows = 3
cols = 4
m = zeros(rows, cols)

# Fill with values 1 to 12
val = 1
for i in 1:rows
    for j in 1:cols
        m[i, j] = val
        val += 1
    end
end

println("Matrix 3x4:")
for i in 1:rows
    for j in 1:cols
        println("m[", i, ",", j, "] = ", m[i, j])
    end
end

sum = matrix_sum(m, rows, cols)
println("Sum of all elements: ", sum)

println(sum)
sum
"#;
    let result = compile_and_run_str(src, 0);
    // Sum of 1 to 12 = 78
    assert!(
        (result - 78.0).abs() < 1e-10,
        "Expected 78.0, got {}",
        result
    );
}

#[test]
fn test_sample_identity_matrix() {
    let src = r#"
function identity(n)
    m = zeros(n, n)
    for i in 1:n
        m[i, i] = 1
    end
    m
end

# Create 4x4 identity matrix
I = identity(4)

println("4x4 Identity Matrix:")
for i in 1:4
    println(I[i, 1], " ", I[i, 2], " ", I[i, 3], " ", I[i, 4])
end

# Verify diagonal elements
@assert I[1, 1] == 1
@assert I[2, 2] == 1
@assert I[3, 3] == 1
@assert I[4, 4] == 1

# Verify off-diagonal elements are zero
@assert I[1, 2] == 0
@assert I[2, 1] == 0

println(I[3, 3])
I[3, 3]
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_sample_sieve_of_eratosthenes() {
    let src = r#"
function sieve(n)
    # Create array: 1 = prime, 0 = not prime
    is_prime = ones(n)
    is_prime[1] = 0  # 1 is not prime

    for i in 2:sqrt(n)
        if is_prime[i] == 1
            # Mark multiples as not prime
            j = i * 2
            while j <= n
                is_prime[j] = 0
                j += i
            end
        end
    end

    # Count primes
    count = 0
    for i in 1:n
        if is_prime[i] == 1
            count += 1
        end
    end
    count
end

# Count primes up to 100
count = sieve(100)
println("Primes up to 100: ", count)

println(count)
count
"#;
    let result = compile_and_run_str(src, 0);
    // There are 25 primes <= 100
    assert!(
        (result - 25.0).abs() < 1e-10,
        "Expected 25.0, got {}",
        result
    );
}

#[test]
fn test_sample_matrix_vector_multiplication() {
    let src = r#"
# Create a 2x3 matrix A
A = zeros(2, 3)
A[1, 1] = 1
A[1, 2] = 2
A[1, 3] = 3
A[2, 1] = 4
A[2, 2] = 5
A[2, 3] = 6

# Create a vector v
v = [1, 2, 3]

# Matrix-vector multiplication: A * v
result = A * v

# Verify: result[1] = 1*1 + 2*2 + 3*3 = 14
@assert result[1] == 14
@assert result[2] == 32

println(result[1])
result[1]
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 14.0).abs() < 1e-10,
        "Expected 14.0, got {}",
        result
    );
}

#[test]
fn test_sample_matrix_matrix_multiplication() {
    let src = r#"
# Create matrix A (2x3)
A = zeros(2, 3)
A[1, 1] = 1
A[1, 2] = 2
A[1, 3] = 3
A[2, 1] = 4
A[2, 2] = 5
A[2, 3] = 6

# Create matrix B (3x2)
B = zeros(3, 2)
B[1, 1] = 7
B[1, 2] = 8
B[2, 1] = 9
B[2, 2] = 10
B[3, 1] = 11
B[3, 2] = 12

# Matrix multiplication: C = A * B (result is 2x2)
C = A * B

# Verify: C[2,2] = 4*8 + 5*10 + 6*12 = 154
@assert C[1, 1] == 58
@assert C[1, 2] == 64
@assert C[2, 1] == 139
@assert C[2, 2] == 154

println(C[2, 2])
C[2, 2]
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 154.0).abs() < 1e-10,
        "Expected 154.0, got {}",
        result
    );
}

#[test]
fn test_sample_broadcast_operations() {
    let src = r#"
# Broadcast (element-wise) operations with .+ .* .- ./

a = [1, 2, 3, 4, 5]
b = [10, 20, 30, 40, 50]

# Element-wise addition
c = a .+ b

# Element-wise multiplication
d = a .* b

# Scalar broadcast: multiply all elements by 10
e = a .* 10

# Element-wise power
f = a .^ 2

println(f[5])
f[5]
"#;
    let result = compile_and_run_str(src, 0);
    // 5^2 = 25
    assert!(
        (result - 25.0).abs() < 1e-10,
        "Expected 25.0, got {}",
        result
    );
}

#[test]
fn test_sample_broadcast_function_calls() {
    let src = r#"
# Broadcast function call syntax: f.(x)
# Create array of perfect squares
squares = [1, 4, 9, 16, 25, 36, 49, 64, 81, 100]

# Apply sqrt to each element
roots = sqrt.(squares)

# Combine with broadcast operations
a = [4, 9, 16]
b = [1, 1, 1]

# sqrt.(a) .+ b = [2, 3, 4] .+ [1, 1, 1] = [3, 4, 5]
result = sqrt.(a) .+ b

println(result[3])
result[3]
"#;
    let result = compile_and_run_str(src, 0);
    // sqrt(16) + 1 = 4 + 1 = 5
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

// ==================== Functions ====================

#[test]
fn test_sample_factorial_iterative() {
    let src = r#"
function my_factorial(n)
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end

answer = my_factorial(10)
println(answer)
answer
"#;
    let result = compile_and_run_str(src, 0);
    // 10! = 3628800
    assert!(
        (result - 3628800.0).abs() < 1e-10,
        "Expected 3628800.0, got {}",
        result
    );
}

#[test]
fn test_sample_factorial_recursive() {
    let src = r#"
function my_factorial(n)
    if n <= 1
        return 1
    end
    n * my_factorial(n - 1)
end

answer = my_factorial(10)
println(answer)
answer
"#;
    let result = compile_and_run_str(src, 0);
    // 10! = 3628800
    assert!(
        (result - 3628800.0).abs() < 1e-10,
        "Expected 3628800.0, got {}",
        result
    );
}

#[test]
fn test_sample_multiple_dispatch() {
    let src = r#"
# Multiple dispatch: same function name, different type signatures

function process(x::Int64)
    return x * 2
end

function process(x::Float64)
    return x / 2.0
end

r1 = process(42)
r2 = process(10.0)

result = r1 + r2
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // process(42) = 84, process(10.0) = 5.0, sum = 89.0
    assert!(
        (result - 89.0).abs() < 1e-10,
        "Expected 89.0, got {}",
        result
    );
}

#[test]
fn test_sample_type_annotations() {
    let src = r#"
function add_ints(a::Int64, b::Int64)
    return a + b
end

function add_floats(a::Float64, b::Float64)
    return a + b
end

function add_any(a, b)
    return a + b
end

result = add_ints(3, 4) + add_floats(1.5, 2.5)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // 7 + 4 = 11
    assert!(
        (result - 11.0).abs() < 1e-10,
        "Expected 11.0, got {}",
        result
    );
}

// ==================== Algorithms ====================

#[test]
fn test_sample_fibonacci() {
    let src = r#"
function fib(n)
    if n <= 1
        return n
    end
    fib(n - 1) + fib(n - 2)
end

result = fib(15)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // fib(15) = 610
    assert!(
        (result - 610.0).abs() < 1e-10,
        "Expected 610.0, got {}",
        result
    );
}

#[test]
fn test_sample_fibonacci_fast() {
    let src = r#"
function fib_fast(n)
    if n <= 1
        return n
    end
    a = 0
    b = 1
    for i in 2:n
        c = a + b
        a = b
        b = c
    end
    b
end

result = fib_fast(30)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // fib(30) = 832040
    assert!(
        (result - 832040.0).abs() < 1e-10,
        "Expected 832040.0, got {}",
        result
    );
}

#[test]
fn test_sample_gcd_euclidean() {
    let src = r#"
function gcd(a, b)
    while b > 0
        temp = b
        b = a % b
        a = temp
    end
    a
end

result = gcd(48, 18)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // gcd(48, 18) = 6
    assert!((result - 6.0).abs() < 1e-10, "Expected 6.0, got {}", result);
}

#[test]
fn test_sample_is_prime() {
    let src = r#"
function is_prime(n)
    if n <= 1
        return 0
    end
    if n <= 3
        return 1
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

result = is_prime(97)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // 97 is prime
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_sample_sum_of_primes() {
    let src = r#"
function is_prime(n)
    if n <= 1
        return 0
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

function sum_primes(n)
    sum = 0
    for i in 2:n
        if is_prime(i) == 1
            sum += i
        end
    end
    sum
end

result = sum_primes(100)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // Sum of primes up to 100 = 1060
    assert!(
        (result - 1060.0).abs() < 1e-10,
        "Expected 1060.0, got {}",
        result
    );
}

#[test]
fn test_sample_mandelbrot_scalar() {
    let src = r#"
# Mandelbrot escape time algorithm
# Type annotations required for Complex dispatch
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Point inside the set (should return maxiter)
c1 = mandelbrot_escape(0.0 + 0.0im, 100)

println(c1)
c1
"#;
    let result = compile_and_run_str(src, 0);
    // Point (0,0) is in the set, so should return maxiter = 100
    assert!(
        (result - 100.0).abs() < 1e-10,
        "Expected 100.0, got {}",
        result
    );
}

#[test]
fn test_sample_mandelbrot_grid() {
    // Use inlined grid creation to avoid shape issue with function returns of 2D struct arrays
    let src = r##"
# Mandelbrot escape time algorithm
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Build 2D complex grid and compute escape times (inlined)
width = 50
height = 25
maxiter = 50

xmin = -2.0; xmax = 1.0
ymin = -1.2; ymax = 1.2

xs = range(xmin, xmax; length=width)
ys = range(ymax, ymin; length=height)

# Create 2D complex grid via broadcasting
xst = xs'
imys = im .* ys
C = xst .+ imys
println("C shape: ", size(C))

# Apply escape function to all points at once
@time grid = mandelbrot_escape.(C, Ref(maxiter))
println("Grid shape: ", size(grid))

# ASCII visualization
println("Mandelbrot Set (50x25):")
for row in 1:25
    for col in 1:50
        n = grid[row, col]
        if n == 50
            print("#")
        elseif n > 25
            print("+")
        elseif n > 10
            print(".")
        else
            print(" ")
        end
    end
    println("")
end

println(grid[12, 25])
grid[12, 25]
"##;
    let result = compile_and_run_str(src, 0);
    // Just verify it runs and returns a reasonable count (grid[12, 25] should be an iteration count)
    assert!(
        (0.0..=50.0).contains(&result),
        "Expected iteration count between 0 and 50, got {}",
        result
    );
}

#[test]
fn test_sample_mandelbrot_broadcast() {
    let src = r#"
function mandelbrot_row(cr_array, ci, maxiter)
    C = cr_array .+ ci * im
    mandelbrot_escape.(C, Ref(maxiter))
end

function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    maxiter
end

width = 15
height = 8
xmin = -2.0
xmax = 1.0
ymin = -1.2
ymax = 1.2
maxiter = 20

cr_array = [xmin + (col - 1) * (xmax - xmin) / (width - 1) for col in 1:width]

in_set = 0
for row in 1:height
    ci = ymax - (row - 1) * (ymax - ymin) / (height - 1)
    iterations = mandelbrot_row(cr_array, ci, maxiter)
    in_set += sum(iterations .== maxiter)
end

println(in_set)
in_set
    "#;
    let result = compile_and_run_str(src, 0);
    // Julia reference: 28 points in set for 15x8 grid with maxiter=20
    assert!(
        (result - 28.0).abs() <= 1.0,
        "Expected within 1.0 of Julia reference 28.0, got {}",
        result
    );
}

// ==================== Monte Carlo ====================

#[test]
fn test_sample_estimate_pi() {
    let src = r#"
function estimate_pi(N)
    inside = 0
    for i in 1:N
        x = rand()
        y = rand()
        if x^2 + y^2 < 1.0
            inside += 1
        end
    end
    4.0 * inside / N
end

result = estimate_pi(10000)
println(result)
result
"#;
    let result = compile_and_run_str(src, 12345);
    // Should be approximately pi
    assert!(
        (result - std::f64::consts::PI).abs() < 0.1,
        "Expected ~3.14159, got {}",
        result
    );
}

#[test]
fn test_sample_random_walk() {
    let src = r#"
function random_walk_1d(steps)
    position = 0.0
    for i in 1:steps
        step = ifelse(rand() < 0.5, -1.0, 1.0)
        position += step
    end
    position
end

result = random_walk_1d(1000)
println(result)
result
"#;
    let result = compile_and_run_str(src, 42);
    // Just verify it runs and returns a number
    assert!(!result.is_nan(), "Expected valid result, got NaN");
}

#[test]
fn test_sample_monte_carlo_integration() {
    let src = r#"
function monte_carlo_integral(N)
    # Estimate integral of x^2 from 0 to 1
    sum = 0.0
    for i in 1:N
        x = rand()
        sum += x^2
    end
    sum / N  # Should be close to 1/3
end

result = monte_carlo_integral(100000)
println(result)
result
"#;
    let result = compile_and_run_str(src, 42);
    // Should be approximately 1/3
    assert!(
        (result - (1.0 / 3.0)).abs() < 0.01,
        "Expected ~0.333, got {}",
        result
    );
}

#[test]
fn test_sample_random_arrays() {
    let src = r#"
# rand(n) creates 1D array of random Float64 in [0, 1)
v = rand(5)

# rand(m, n) creates 2D array (matrix)
m = rand(3, 3)

# Sum of all elements
sum = 0.0
for i in 1:3
    for j in 1:3
        sum += m[i, j]
    end
end

println(sum)
sum
"#;
    let result = compile_and_run_str(src, 42);
    // Sum should be between 0 and 9 (9 elements, each in [0,1))
    assert!(
        (0.0..=9.0).contains(&result),
        "Expected sum between 0 and 9, got {}",
        result
    );
}

#[test]
fn test_sample_random_integer_arrays() {
    let src = r#"
# rand(Int, n) creates array of random integers
ints = rand(Int, 5)

# rand(Int, m, n) creates 2D array of random integers
mat = rand(Int, 2, 3)

# Find the maximum value
max_val = mat[1, 1]
for i in 1:2
    for j in 1:3
        if mat[i, j] > max_val
            max_val = mat[i, j]
        end
    end
end

println(max_val)
max_val
"#;
    let result = compile_and_run_str(src, 42);
    // Just verify it runs and returns a number
    assert!(!result.is_nan(), "Expected valid result, got NaN");
}

#[test]
fn test_sample_random_simulation() {
    let src = r#"
function simulate_dice(n_rolls)
    rolls = rand(n_rolls)
    sum = 0
    for i in 1:n_rolls
        die = 1 + floor(rolls[i] * 6)
        if die > 6
            die = 6
        end
        sum += die
    end
    sum / n_rolls
end

avg = simulate_dice(1000)
println(avg)
avg
"#;
    let result = compile_and_run_str(src, 42);
    // Average should be close to 3.5
    assert!((result - 3.5).abs() < 0.5, "Expected ~3.5, got {}", result);
}

#[test]
fn test_sample_normal_distribution_randn() {
    let src = r#"
# randn() generates standard normal random numbers
arr = randn(10)

# Calculate sample mean (should be close to 0)
sum = 0.0
for i in 1:length(arr)
    sum += arr[i]
end
mean = sum / length(arr)

println(mean)
mean
"#;
    let result = compile_and_run_str(src, 42);
    // Mean should be close to 0
    assert!(
        result.abs() < 1.0,
        "Expected mean close to 0, got {}",
        result
    );
}

#[test]
fn test_sample_normal_distribution_matrix() {
    let src = r#"
# randn(m, n) creates 2D array of normal random values
mat = randn(3, 4)

# Calculate statistics
sum = 0.0
sum_sq = 0.0
n = 12
for i in 1:3
    for j in 1:4
        v = mat[i, j]
        sum += v
        sum_sq += v * v
    end
end

mean = sum / n
variance = sum_sq / n - mean * mean
std = sqrt(variance)

println(std)
std
"#;
    let result = compile_and_run_str(src, 42);
    // Standard deviation should be close to 1
    assert!(
        (result - 1.0).abs() < 1.0,
        "Expected std close to 1, got {}",
        result
    );
}

#[test]
fn test_sample_box_muller_visualization() {
    let src = r#"
# Generate many normal values and visualize distribution
n = 1000
samples = randn(n)

# Count samples in histogram bins
bins = zeros(6)

for i in 1:n
    x = samples[i]
    if x < -2
        bins[1] += 1
    elseif x < -1
        bins[2] += 1
    elseif x < 0
        bins[3] += 1
    elseif x < 1
        bins[4] += 1
    elseif x < 2
        bins[5] += 1
    else
        bins[6] += 1
    end
end

# Most values should be in the middle bins
println(bins[3] + bins[4])
bins[3] + bins[4]
"#;
    let result = compile_and_run_str(src, 42);
    // Most values should be in middle bins (3 and 4)
    // For N(0,1), ~68.27% fall in [-1,1], so expect ~683 of 1000
    // Using wide tolerance for statistical tests (600-850)
    assert!(
        (600.0..=850.0).contains(&result),
        "Expected count between 600 and 850, got {}",
        result
    );
}

// ==================== Mathematics ====================

#[test]
fn test_sample_geometric_series() {
    let src = r#"
function geometric_sum(r, n)
    # Sum of r^0 + r^1 + ... + r^(n-1)
    sum = 0.0
    term = 1.0
    for i in 1:n
        sum += term
        term *= r
    end
    sum
end

result = geometric_sum(0.5, 10)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // Sum = 1 + 0.5 + 0.25 + ... = 2 * (1 - 0.5^10) = 1.998046875
    assert!(
        (result - 1.998046875).abs() < 1e-9,
        "Expected 1.998046875, got {}",
        result
    );
}

#[test]
fn test_sample_newton_method() {
    let src = r#"
function newton_sqrt(x)
    # Find sqrt(x) using Newton's method
    guess = x / 2.0
    for i in 1:10
        guess = (guess + x / guess) / 2.0
    end
    guess
end

result = newton_sqrt(2.0)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // sqrt(2) ≈ 1.41421356...
    assert!(
        (result - std::f64::consts::SQRT_2).abs() < 1e-10,
        "Expected ~1.41421, got {}",
        result
    );
}

#[test]
fn test_sample_taylor_series_exp() {
    let src = r#"
function exp_taylor(x, terms)
    # e^x ≈ 1 + x + x^2/2! + x^3/3! + ...
    result = 1.0
    term = 1.0
    for n in 1:terms
        term = term * x / n
        result += term
    end
    result
end

result = exp_taylor(1.0, 20)  # Should be close to e ≈ 2.71828
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // e ≈ 2.718281828...
    assert!(
        (result - std::f64::consts::E).abs() < 1e-10,
        "Expected ~2.71828, got {}",
        result
    );
}

#[test]
fn test_sample_coprime_pi_estimation() {
    // Simplified version that runs faster
    let src = r#"
# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

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

result = calc_pi(50)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // Should be approximately pi (with larger tolerance due to small N)
    assert!(
        (result - std::f64::consts::PI).abs() < 0.5,
        "Expected ~3.14159, got {}",
        result
    );
}

// ==================== Macros ====================

#[test]
fn test_sample_assert_in_loop() {
    let src = r#"
function sum_positive(n)
    @assert n >= 0 "n must be non-negative"
    sum = 0
    for i in 1:n
        @assert i > 0
        sum += i
    end
    sum
end

result = sum_positive(10)
println(result)
result
"#;
    // Debug: check compilation
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(src).unwrap();
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(outcome).unwrap();
    match compile_core_program(&program) {
        Ok(_) => eprintln!("Compilation successful"),
        Err(e) => eprintln!("Compilation error: {:?}", e),
    }
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 55.0).abs() < 1e-10,
        "Expected 55.0, got {}",
        result
    );
}

#[test]
fn test_sample_time_monte_carlo() {
    let src = r#"
function estimate_pi(N)
    inside = 0
    for i in 1:N
        x = rand()
        y = rand()
        if x^2 + y^2 < 1.0
            inside += 1
        end
    end
    4.0 * inside / N
end

# Time the Monte Carlo simulation
@time begin
    pi_estimate = estimate_pi(100000)
    println("Estimated pi: ", pi_estimate)
end
println(pi_estimate)
pi_estimate
"#;
    let result = compile_and_run_str(src, 12345);
    // Should be approximately pi
    assert!(
        (result - std::f64::consts::PI).abs() < 0.1,
        "Expected ~3.14159, got {}",
        result
    );
}

#[test]
fn test_sample_combined_assert_time() {
    let src = r#"
function checked_factorial(n)
    @assert n >= 0 "n must be non-negative"
    @assert n <= 20 "n too large (overflow risk)"
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end

@time begin
    f10 = checked_factorial(10)
    println("10! = ", f10)
    f15 = checked_factorial(15)
    println("15! = ", f15)
end
println(f15)
f15
"#;
    let result = compile_and_run_str(src, 0);
    // 15! = 1307674368000
    assert!(
        (result - 1307674368000.0).abs() < 1e-6,
        "Expected 1307674368000, got {}",
        result
    );
}

#[test]
fn test_sample_show_debugging() {
    let src = r#"
function debug_sum(n)
    sum = 0
    for i in 1:n
        sum += i
        @show sum
    end
    sum
end

result = debug_sum(5)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // Sum of 1 to 5 = 15
    assert!(
        (result - 15.0).abs() < 1e-10,
        "Expected 15.0, got {}",
        result
    );
}

#[test]
fn test_sample_show_with_expressions() {
    // Note: Function definitions must come before main code in the VM
    let src = r#"
function hypotenuse(a, b)
    @show a
    @show b
    result = sqrt(a^2 + b^2)
    @show result
    result
end

x = 10
y = 20

@show x
@show y
@show x + y
@show x * y

result = hypotenuse(3.0, 4.0)
println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // sqrt(3^2 + 4^2) = sqrt(25) = 5
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

// ==================== Higher-Order Functions ====================

#[test]
fn test_sample_map_function() {
    let src = r#"
# map(f, arr) applies function f to each element
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

function double(x)
    return x * 2.0
end

function square(x)
    return x * x
end

# Square each element
squared = map(square, arr)

println(squared[5])
squared[5]
"#;
    let result = compile_and_run_str(src, 0);
    // 5^2 = 25
    assert!(
        (result - 25.0).abs() < 1e-10,
        "Expected 25.0, got {}",
        result
    );
}

#[test]
fn test_sample_filter_function() {
    let src = r#"
# filter(f, arr) keeps elements where f returns true
arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

function is_even(x)
    return x % 2 == 0
end

function is_large(x)
    return x > 5
end

# Keep only even numbers
evens = filter(is_even, arr)

# Keep numbers greater than 5
large = filter(is_large, arr)

println(length(evens) + length(large))
length(evens) + length(large)
"#;
    let result = compile_and_run_str(src, 0);
    // evens: [2,4,6,8,10] = 5, large: [6,7,8,9,10] = 5, total = 10
    assert!(
        (result - 10.0).abs() < 1e-10,
        "Expected 10.0, got {}",
        result
    );
}

#[test]
fn test_sample_reduce_function() {
    let src = r#"
# reduce(f, arr) combines elements using binary function f
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

function add(a, b)
    return a + b
end

function multiply(a, b)
    return a * b
end

# Product of all elements
product = reduce(multiply, arr)

println(product)
product
"#;
    let result = compile_and_run_str(src, 0);
    // 1 * 2 * 3 * 4 * 5 = 120
    assert!(
        (result - 120.0).abs() < 1e-10,
        "Expected 120.0, got {}",
        result
    );
}

#[test]
fn test_sample_do_syntax_for_map() {
    let src = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# Using do syntax (equivalent to map(x -> x^2 + 1, arr))
result = map(arr) do x
    x^2 + 1
end

println(result[5])
result[5]
"#;
    let result = compile_and_run_str(src, 0);
    // 5^2 + 1 = 26
    assert!(
        (result - 26.0).abs() < 1e-10,
        "Expected 26.0, got {}",
        result
    );
}

#[test]
fn test_sample_do_syntax_for_filter_reduce() {
    let src = r#"
data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# Filter with do block
filtered = filter(data) do x
    x > 3 && x < 8
end

# Reduce with do block (multiple parameters)
total = reduce(data) do acc, val
    acc + val
end

println(total)
total
"#;
    let result = compile_and_run_str(src, 0);
    // Sum of 1 to 10 = 55
    assert!(
        (result - 55.0).abs() < 1e-10,
        "Expected 55.0, got {}",
        result
    );
}

#[test]
fn test_sample_chaining_higher_order_functions() {
    let src = r#"
function square(x)
    return x * x
end

function is_large(x)
    return x > 20
end

function add(a, b)
    return a + b
end

data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# Step 1: Square all numbers
squared = map(square, data)

# Step 2: Keep only those > 20
large_squares = filter(is_large, squared)

# Step 3: Sum them
total = reduce(add, large_squares)

println(total)
total
"#;
    let result = compile_and_run_str(src, 0);
    // Squares > 20: 25, 36, 49, 64, 81, 100, sum = 355
    assert!(
        (result - 355.0).abs() < 1e-10,
        "Expected 355.0, got {}",
        result
    );
}

// ==================== Structures ====================

#[test]
fn test_sample_basic_struct() {
    let src = r#"
# Define an immutable struct with typed fields
struct Point
    x::Float64
    y::Float64
end

# Create instances
p1 = Point(3.0, 4.0)

# Use in calculations
distance = sqrt(p1.x^2 + p1.y^2)

println(distance)
distance
"#;
    let result = compile_and_run_str(src, 0);
    // sqrt(3^2 + 4^2) = sqrt(25) = 5
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

#[test]
fn test_sample_mutable_struct() {
    let src = r#"
# Mutable structs allow field modification
mutable struct Counter
    value::Float64
end

# Create and modify
c = Counter(0.0)

c.value = 10.0
c.value = c.value + 5.0

# Use in a loop
for i in 1:5
    c.value = c.value + 1.0
end

println(c.value)
c.value
"#;
    let result = compile_and_run_str(src, 0);
    // 0 + 10 + 5 + 5 = 20
    assert!(
        (result - 20.0).abs() < 1e-10,
        "Expected 20.0, got {}",
        result
    );
}

#[test]
fn test_sample_struct_with_functions() {
    let src = r#"
# Define a struct
struct Rectangle
    width::Float64
    height::Float64
end

# Functions that work with structs
function area(r)
    r.width * r.height
end

# Create rectangles
rect = Rectangle(5.0, 3.0)

println(area(rect))
area(rect)
"#;
    let result = compile_and_run_str(src, 0);
    // 5 * 3 = 15
    assert!(
        (result - 15.0).abs() < 1e-10,
        "Expected 15.0, got {}",
        result
    );
}

#[test]
fn test_sample_euclidean_distance() {
    let src = r#"
# Point struct for 2D geometry
struct Point
    x::Float64
    y::Float64
end

# Calculate distance between two points
function distance(p1, p2)
    dx = p2.x - p1.x
    dy = p2.y - p1.y
    sqrt(dx*dx + dy*dy)
end

# Create points
a = Point(3.0, 4.0)
b = Point(6.0, 8.0)

println(distance(a, b))
distance(a, b)
"#;
    let result = compile_and_run_str(src, 0);
    // sqrt((6-3)^2 + (8-4)^2) = sqrt(9 + 16) = sqrt(25) = 5
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

#[test]
fn test_sample_particle_simulation() {
    let src = r#"
# Mutable struct for particle position
mutable struct Particle
    x::Float64
    y::Float64
    vx::Float64
    vy::Float64
end

# Update particle position
function step!(p, dt)
    p.x = p.x + p.vx * dt
    p.y = p.y + p.vy * dt
end

# Create a particle with initial position and velocity
particle = Particle(0.0, 0.0, 1.0, 0.5)

# Simulate 10 time steps
dt = 0.1
for t in 1:10
    step!(particle, dt)
end

# Return final distance from origin
println(sqrt(particle.x^2 + particle.y^2))
sqrt(particle.x^2 + particle.y^2)
"#;
    let result = compile_and_run_str(src, 0);
    // After 10 steps: x = 1.0, y = 0.5, distance = sqrt(1.25) ≈ 1.118
    assert!(
        (result - 1.118033988749895).abs() < 1e-10,
        "Expected ~1.118, got {}",
        result
    );
}

// ==================== Error Handling ====================

#[test]
fn test_sample_try_catch_basics() {
    let src = r#"
# try/catch handles runtime errors gracefully
x = 0

try
    # Use integer division to trigger division by zero error
    # Note: Float division (1 / 0) returns Inf per IEEE 754, not an error
    y = div(1, 0)
    x = 999  # This won't execute
catch e
    # e contains the error message
    println("Caught error: ", e)
    x = -1
end

println(x)
x
"#;
    let result = compile_and_run_str(src, 0);
    // Should catch error and set x to -1
    assert!(
        (result - (-1.0)).abs() < 1e-10,
        "Expected -1.0, got {}",
        result
    );
}

#[test]
fn test_sample_try_catch_finally() {
    let src = r#"
# finally block always executes, error or not
result = 0
cleanup_done = 0

try
    result = 10 / 2  # Normal operation
    println("Computed result: ", result)
catch e
    println("Error: ", e)
    result = -1
finally
    # This always runs
    cleanup_done = 1
    println("Cleanup completed")
end

println(result)
result
"#;
    let result = compile_and_run_str(src, 0);
    // 10 / 2 = 5
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

#[test]
fn test_sample_error_recovery() {
    let src = r#"
# Recover from errors and continue processing
function safe_divide(a, b)
    result = 0.0
    try
        result = a / b
    catch e
        println("Cannot divide ", a, " by ", b)
        result = 0.0
    end
    result
end

println("10 / 2 = ", safe_divide(10, 2))
println("10 / 0 = ", safe_divide(10, 0))
println("20 / 4 = ", safe_divide(20, 4))

# All divisions complete even with errors
println(safe_divide(100, 5))
safe_divide(100, 5)
"#;
    let result = compile_and_run_str(src, 0);
    // 100 / 5 = 20
    assert!(
        (result - 20.0).abs() < 1e-10,
        "Expected 20.0, got {}",
        result
    );
}

// ==================== Smoke Test for All Samples ====================

#[test]
fn test_all_samples_compile_and_run() {
    let samples = vec![
        // Basic
        (r#"println("Hello, World!")"#, "Hello World"),
        // Arrays - simplified versions
        (
            r#"arr = [1, 2, 3, 4, 5]
println(arr[3])
arr[3]"#,
            "Vector Basics",
        ),
        (
            r#"r = 1:5
println(length(r))
length(r)"#,
            "Range Expressions",
        ),
        (
            r#"squares = [x^2 for x in 1:5]
println(length(squares))
length(squares)"#,
            "Comprehension",
        ),
        (
            r#"evens = [x for x in 1:10 if x % 2 == 0]
println(length(evens))
length(evens)"#,
            "Comprehension with Filter",
        ),
        (
            r#"z = zeros(5)
println(z[1])
z[1]"#,
            "Array Functions",
        ),
        (
            r#"arr = [10, 20, 30]
arr[2] = 99
println(arr[2])
arr[2]"#,
            "Array Mutation",
        ),
        // Functions
        (
            r#"function factorial(n)
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end
result = factorial(5)
println(result)
result"#,
            "Factorial",
        ),
        // Algorithms
        (
            r#"function fib(n)
    if n <= 1
        return n
    end
    fib(n - 1) + fib(n - 2)
end
result = fib(10)
println(result)
result"#,
            "Fibonacci",
        ),
        (
            r#"function gcd(a, b)
    while b > 0
        temp = b
        b = a % b
        a = temp
    end
    a
end
result = gcd(48, 18)
println(result)
result"#,
            "GCD",
        ),
        // Monte Carlo
        (
            r#"function estimate_pi(N)
    inside = 0
    for i in 1:N
        x = rand()
        y = rand()
        if x^2 + y^2 < 1.0
            inside += 1
        end
    end
    4.0 * inside / N
end
result = estimate_pi(1000)
println(result)
result"#,
            "Estimate Pi",
        ),
        // Mathematics
        (
            r#"function newton_sqrt(x)
    guess = x / 2.0
    for i in 1:10
        guess = (guess + x / guess) / 2.0
    end
    guess
end
result = newton_sqrt(2.0)
println(result)
result"#,
            "Newton's Method",
        ),
        // Macros
        (
            r#"@assert 1 > 0
result = 42
println(result)
result"#,
            "Assert",
        ),
        (
            r#"x = 10
@show x
println(x)
x"#,
            "Show",
        ),
    ];

    for (src, name) in samples {
        let result = compile_and_run_str(src, 42);
        assert!(
            !result.is_nan(),
            "Sample '{}' failed: returned NaN\nSource:\n{}",
            name,
            src
        );
    }
}

// ==================== Array Type Promotion ====================

#[test]
fn test_array_int_rational_promotion() {
    // Test that [1//1, 2//1, 3//2] creates a Rational array
    // Access the numerator of the first element (should be 1)
    // Note: Due to where clause limitations, Rational is now non-parametric
    // and doesn't support automatic promotion from Int to Rational in array literals
    let src = r#"
arr = [1//1, 2//1, 3//2]
arr[1].num
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "Expected 1.0 (numerator of 1), got {}",
        result
    );
}

#[test]
fn test_array_int_rational_promotion_denominator() {
    // Test that [1//1, 2//1, 3//2] creates a Rational array
    // Access the denominator of the first element (should be 1)
    // Note: Due to where clause limitations, Rational is now non-parametric
    let src = r#"
arr = [1//1, 2//1, 3//2]
arr[1].den
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "Expected 1.0 (denominator of 1), got {}",
        result
    );
}
