// IR JSON for web playground samples
// Samples with ir: null use CST JSON + WASM-side lowering
// Synced from iOS app (SubsetJuliaVMApp) - Beginner, Intermediate, and Advanced samples

export const samplesIR = [
  // ==================== BEGINNER ====================
  {
    name: "Hello World",
    code: `#=
==========================================
Welcome to SubsetJuliaVM Playground!
==========================================

HOW TO USE:
1. Select a sample from the dropdown menu
2. Edit the Julia code in this editor
3. Click "Run" or press Ctrl+Enter to execute
4. View results in the Output panel on the right
5. Use "Copy" to copy code or output

SHORTCUTS:
- Ctrl+Enter (or Cmd+Enter): Run code

==========================================
SubsetJuliaVM Playground へようこそ！
==========================================

使い方:
1. ドロップダウンからサンプルを選択
2. エディタで Julia コードを編集
3. 「Run」をクリック、または Ctrl+Enter で実行
4. 右側の Output パネルで結果を確認
5. 「Copy」でコードや出力をコピー

ショートカット:
- Ctrl+Enter (または Cmd+Enter): 実行
==========================================
=#

println("Hello, World!")`,
    ir: null
  },
  {
    name: "String Interpolation",
    code: `# String interpolation with $(expression)
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

println(x)`,
    ir: null
  },
  {
    name: "Print Smile",
    code: `function print_smile()
    println("  ____  ")
    println(" /    \\\\ ")
    println("|  ^ ^ |")
    println("|      |")
    println("|  \\\\_/ |")
    println("|      |")
    println(" \\\\____/ ")
    println("        ")
end

print_smile()`,
    ir: null
  },
  {
    name: "Vector Basics",
    code: `# Create a vector
arr = [1, 2, 3, 4, 5]

# Access elements (1-indexed like Julia)
println("First element: ", arr[1])
println("Third element: ", arr[3])
println("Last element: ", arr[5])

# Get length
println("Length: ", length(arr))

println(arr[3])`,
    ir: null
  },
  {
    name: "Range Expressions",
    code: `# Range 1:5 creates [1, 2, 3, 4, 5]
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

println(sum)`,
    ir: null
  },
  {
    name: "Array Functions",
    code: `# zeros(n) - create array of n zeros
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

println(powers_of_2[11])`,
    ir: null
  },
  {
    name: "@show Debugging",
    code: `# @show prints "expr = value" format
x = 10
y = 20

@show x
@show y
@show x + y
@show x * y

# Useful for debugging calculations
function hypotenuse(a, b)
    @show a
    @show b
    result = sqrt(a^2 + b^2)
    @show result
    result
end

# Use in a loop for tracing
function debug_sum(n)
    sum = 0
    for i in 1:n
        sum += i
        @show sum
    end
    sum
end

println("Hypotenuse: ", hypotenuse(3.0, 4.0))
println("Sum: ", debug_sum(5))`,
    ir: null
  },

  // ==================== INTERMEDIATE ====================
  {
    name: "FizzBuzz",
    code: `function fizzbuzz(n)
    for i in 1:n
        if i % 15 == 0
            println("FizzBuzz")
        elseif i % 3 == 0
            println("Fizz")
        elseif i % 5 == 0
            println("Buzz")
        else
            println(i)
        end
    end
end

fizzbuzz(100)`,
    ir: null
  },
  {
    name: "Array Comprehension",
    code: `# Basic comprehension: [expr for var in iter]
squares = [x^2 for x in 1:5]
println("Squares of 1 to 5:")
for i in 1:length(squares)
    println("  ", i, "^2 = ", squares[i])
end

# Comprehension with filter: [expr for var in iter if cond]
evens = [x for x in 1:10 if x % 2 == 0]
println("Even numbers from 1 to 10:")
for i in 1:length(evens)
    println("  ", evens[i])
end

# Squares of odd numbers only
odd_squares = [x^2 for x in 1:10 if x % 2 == 1]
println("Squares of odd numbers:")
for i in 1:length(odd_squares)
    println("  ", odd_squares[i])
end

println(length(evens) + length(odd_squares))`,
    ir: null
  },
  {
    name: "Array Mutation",
    code: `# Start with an array
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

println(arr[2])`,
    ir: null
  },
  {
    name: "Dot Product",
    code: `function dot_product(a, b)
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

println(result)`,
    ir: null
  },
  {
    name: "Statistical Functions",
    code: `function array_sum(arr)
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

println(array_mean(data))`,
    ir: null
  },
  {
    name: "2D Matrix Operations",
    code: `# Create and fill a 3x3 matrix
m = zeros(3, 3)
for i in 1:3
    for j in 1:3
        m[i, j] = i * 10 + j
    end
end

println("3x3 Matrix:")
for i in 1:3
    println(m[i, 1], " ", m[i, 2], " ", m[i, 3])
end

# Different initialization methods
z = zeros(2, 3)  # all zeros
o = ones(2, 3)   # all ones
f = fill(7, 2, 3) # custom value

println("Ones 2x3:")
println(o[1, 1], " ", o[1, 2], " ", o[1, 3])

# Matrix sum function
function matrix_sum(mat, rows, cols)
    sum = 0.0
    for i in 1:rows
        for j in 1:cols
            sum += mat[i, j]
        end
    end
    sum
end

println("Sum of 3x3 matrix: ", matrix_sum(m, 3, 3))
println(m[2, 3])`,
    ir: null
  },
  {
    name: "Identity Matrix",
    code: `function identity(n)
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

println(I[3, 3])`,
    ir: null
  },
  {
    name: "Matrix Multiplication",
    code: `# Matrix-Vector Multiplication
A = zeros(2, 3)
A[1, 1] = 1; A[1, 2] = 2; A[1, 3] = 3
A[2, 1] = 4; A[2, 2] = 5; A[2, 3] = 6

v = [1, 2, 3]
result_v = A * v
println("Matrix A (2x3) * vector v:")
println("  result[1] = ", result_v[1], " (expected 14)")
println("  result[2] = ", result_v[2], " (expected 32)")

# Matrix-Matrix Multiplication
B = zeros(3, 2)
B[1, 1] = 7; B[1, 2] = 8
B[2, 1] = 9; B[2, 2] = 10
B[3, 1] = 11; B[3, 2] = 12

C = A * B  # Result is 2x2
println("Matrix A (2x3) * Matrix B (3x2):")
println("  C[1,1] = ", C[1, 1], " (expected 58)")
println("  C[1,2] = ", C[1, 2], " (expected 64)")
println("  C[2,1] = ", C[2, 1], " (expected 139)")
println("  C[2,2] = ", C[2, 2], " (expected 154)")

println(C[2, 2])`,
    ir: null
  },
  {
    name: "Linear Algebra (SVD)",
    code: `using LinearAlgebra
using Test

A = rand(3, 3)
y = rand(3)
x = A \ y
@test y ≈ A * x

A = rand(3, 4)
F = svd(A)
U, S, V = F
@test V' ≈ F.Vt
@test A ≈ U * Diagonal(S) * V'

true`,
    ir: null
  },
  {
    name: "Broadcast Operations",
    code: `# Element-wise operations with .+ .* .- ./ .^
a = [1, 2, 3, 4, 5]
b = [10, 20, 30, 40, 50]

# Element-wise operations
c = a .+ b
d = a .* b
e = a .* 10  # scalar broadcast
f = a .^ 2   # element-wise power

println("a .+ b = ", c[1], ", ", c[2], ", ...")
println("a .* b = ", d[1], ", ", d[2], ", ...")
println("a .* 10 = ", e[1], ", ", e[2], ", ...")
println("a .^ 2 = ", f[1], ", ", f[2], ", ...")

# Broadcast function call: f.(x)
squares = [1, 4, 9, 16, 25]
roots = sqrt.(squares)
println("sqrt.([1,4,9,16,25]) = ", roots[1], ", ", roots[2], ", ...")

println(f[5])`,
    ir: null
  },
  {
    name: "Multiplication Table",
    code: `# Create 9x9 multiplication table using broadcast outer product
table = .*((1:9)', 1:9)

println("9x9 Multiplication Table:")
println("")
for row in 1:9
    for col in 1:9
        val = table[row, col]
        if val < 10
            print(" ")
        end
        print(val)
        print(" ")
    end
    println("")
end

println("")
println("7 x 8 = ", table[7, 8])
println("9 x 9 = ", table[9, 9])

println(table[9, 9])`,
    ir: null
  },
  {
    name: "Factorial",
    code: `# Iterative implementation
function factorial_iter(n)
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end

# Recursive implementation
function factorial_rec(n)
    if n <= 1
        return 1
    end
    n * factorial_rec(n - 1)
end

println("Iterative: 10! = ", factorial_iter(10))
println("Recursive: 10! = ", factorial_rec(10))

# Verify they give the same result
@assert factorial_iter(10) == factorial_rec(10)

println(factorial_iter(10))`,
    ir: null
  },
  {
    name: "Multiple Dispatch",
    code: `# Multiple dispatch: same function name, different type signatures

# Method for integers
function process(x::Int64)
    println("Integer method: ", x, " → ", x * 2)
    return x * 2
end

# Method for floats
function process(x::Float64)
    println("Float method: ", x, " → ", x / 2.0)
    return x / 2.0
end

# Integer literals dispatch to Int64 method
r1 = process(42)

# Float literals dispatch to Float64 method
r2 = process(10.0)

println("Results: ", r1, ", ", r2)
println(r1)`,
    ir: null
  },
  {
    name: "Type Annotations",
    code: `# Type annotations ensure type safety
function add_ints(a::Int64, b::Int64)
    return a + b
end

function add_floats(a::Float64, b::Float64)
    return a + b
end

# Untyped parameters accept any type
function add_any(a, b)
    return a + b
end

println("add_ints(3, 4) = ", add_ints(3, 4))
println("add_floats(1.5, 2.5) = ", add_floats(1.5, 2.5))
println("add_any(10, 20) = ", add_any(10, 20))

println(add_ints(3, 4) + add_floats(1.5, 2.5))`,
    ir: null
  },
  {
    name: "Fibonacci",
    code: `# Recursive (simple but slow for large n)
function fib_slow(n)
    if n <= 1
        return n
    end
    fib_slow(n - 1) + fib_slow(n - 2)
end

# Iterative (fast)
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

println("Recursive fib(15) = ", fib_slow(15))
println("Iterative fib(30) = ", fib_fast(30))

println(fib_fast(30))`,
    ir: null
  },
  {
    name: "GCD (Euclidean)",
    code: `function gcd(a, b)
    while b > 0
        temp = b
        b = a % b
        a = temp
    end
    a
end

println("gcd(48, 18) = ", gcd(48, 18))
println("gcd(100, 35) = ", gcd(100, 35))

println(gcd(48, 18))`,
    ir: null
  },
  {
    name: "Is Prime",
    code: `function is_prime(n)
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

# Test some numbers
println("is_prime(97) = ", is_prime(97))
println("is_prime(100) = ", is_prime(100))

# Count primes up to 50
count = 0
for i in 2:50
    if is_prime(i) == 1
        count += 1
    end
end
println("Primes up to 50: ", count)

println(is_prime(97))`,
    ir: null
  },
  {
    name: "Estimate Pi (Monte Carlo)",
    code: `function estimate_pi(N)
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

@time pi_est = estimate_pi(100000)
println("Estimated π = ", pi_est)

println(pi_est)`,
    ir: null
  },
  {
    name: "Random Walk",
    code: `function random_walk_1d(steps)
    position = 0.0
    for i in 1:steps
        step = ifelse(rand() < 0.5, -1.0, 1.0)
        position += step
    end
    position
end

println("Random walk results (1000 steps each):")
for trial in 1:5
    result = random_walk_1d(1000)
    println("  Trial ", trial, ": ", result)
end

println(random_walk_1d(1000))`,
    ir: null
  },
  {
    name: "Random Arrays",
    code: `# rand(n) creates 1D array of random Float64 in [0, 1)
v = rand(5)
println("Random vector:")
for i in 1:5
    println("  v[", i, "] = ", v[i])
end

# rand(m, n) creates 2D array
m = rand(3, 3)
println("Random 3x3 matrix:")
for i in 1:3
    println(m[i, 1], " ", m[i, 2], " ", m[i, 3])
end

# rand(Int, n) creates random integers
ints = rand(Int, 5)
println("Random integers:")
for i in 1:3
    println("  ints[", i, "] = ", ints[i])
end

println(length(v) + length(ints))`,
    ir: null
  },
  {
    name: "Normal Distribution (randn)",
    code: `# randn() generates standard normal random numbers (mean=0, std=1)
println("Random normal values:")
for i in 1:5
    println("  ", randn())
end

# randn(n) and randn(m,n) for arrays
arr = randn(100)

# Calculate sample statistics
sum = 0.0
sum_sq = 0.0
for i in 1:length(arr)
    sum += arr[i]
    sum_sq += arr[i] * arr[i]
end
mean = sum / length(arr)
variance = sum_sq / length(arr) - mean * mean
std = sqrt(variance)

println("Sample mean: ", mean, " (expected ~0)")
println("Sample std: ", std, " (expected ~1)")

println(std)`,
    ir: null
  },
  {
    name: "Geometric Series",
    code: `function geometric_sum(r, n)
    # Sum of r^0 + r^1 + ... + r^(n-1)
    sum = 0.0
    term = 1.0
    for i in 1:n
        sum += term
        term *= r
    end
    sum
end

println("Geometric sum (r=0.5, n=10): ", geometric_sum(0.5, 10))
println("Geometric sum (r=0.5, n=20): ", geometric_sum(0.5, 20))
# Converges to 2.0 as n → ∞

println(geometric_sum(0.5, 10))`,
    ir: null
  },
  {
    name: "Macros (@time, @assert)",
    code: `function checked_factorial(n)
    @assert n >= 0 "n must be non-negative"
    @assert n <= 20 "n too large (overflow risk)"
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end

# @time measures execution time
@time begin
    f10 = checked_factorial(10)
    println("10! = ", f10)
    f15 = checked_factorial(15)
    println("15! = ", f15)
end

# @assert validates conditions
@assert f10 == 3628800 "Factorial calculation error"

println(f15)`,
    ir: null
  },
  {
    name: "Map and Filter",
    code: `arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# map(f, arr) applies function to each element
function square(x)
    return x * x
end
squared = map(square, arr)
println("Squares: ", squared[1], ", ", squared[2], ", ...")

# filter(f, arr) keeps elements where f returns true
function is_even(x)
    return x % 2 == 0
end
evens = filter(is_even, arr)
println("Even numbers: ", evens[1], ", ", evens[2], ", ...")

println(length(evens))`,
    ir: null
  },
  {
    name: "Reduce Function",
    code: `arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# reduce(f, arr) combines elements with binary function
function add(a, b)
    return a + b
end

function multiply(a, b)
    return a * b
end

total = reduce(add, arr)
product = reduce(multiply, arr)

println("Sum: ", total)
println("Product: ", product)

# With initial value
total_100 = reduce(add, arr, 100.0)
println("Sum with init=100: ", total_100)

println(product)`,
    ir: null
  },
  {
    name: "Do Syntax",
    code: `arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# do...end creates anonymous function as first argument
result = map(arr) do x
    x^2 + 1
end
println("x^2 + 1: ", result[1], ", ", result[2], ", ...")

# Works with filter too
data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
filtered = filter(data) do x
    x > 3 && x < 7
end
println("Numbers between 3 and 7: ", filtered[1], ", ", filtered[2], ", ...")

# And reduce with multiple parameters
total = reduce(data) do acc, val
    acc + val
end
println("Sum: ", total)

println(total)`,
    ir: null
  },
  {
    name: "Structs",
    code: `# Immutable struct
struct Point
    x::Float64
    y::Float64
end

# Mutable struct
mutable struct Counter
    value::Float64
end

# Functions that work with structs
function distance(p1, p2)
    dx = p2.x - p1.x
    dy = p2.y - p1.y
    sqrt(dx*dx + dy*dy)
end

# Create and use
origin = Point(0.0, 0.0)
p = Point(3.0, 4.0)
println("Distance from origin: ", distance(origin, p))

# Mutable struct can be modified
c = Counter(0.0)
for i in 1:5
    c.value = c.value + 1.0
end
println("Counter value: ", c.value)

println(distance(origin, p))`,
    ir: null
  },
  {
    name: "Struct with Functions",
    code: `struct Rectangle
    width::Float64
    height::Float64
end

function area(r)
    r.width * r.height
end

function perimeter(r)
    2.0 * (r.width + r.height)
end

function is_square(r)
    r.width == r.height
end

# Create rectangles
rect = Rectangle(5.0, 3.0)
square = Rectangle(4.0, 4.0)

println("Rectangle 5x3:")
println("  Area: ", area(rect))
println("  Perimeter: ", perimeter(rect))
println("  Is square: ", is_square(rect))

println("Square 4x4:")
println("  Area: ", area(square))
println("  Is square: ", is_square(square))

println(area(rect))`,
    ir: null
  },
  {
    name: "Try/Catch Error Handling",
    code: `# Safe division with error handling
function safe_divide(a, b)
    result = 0.0
    try
        result = a / b
    catch e
        println("Error: Cannot divide ", a, " by ", b)
        result = 0.0
    finally
        println("Division attempted")
    end
    result
end

println("10 / 2 = ", safe_divide(10, 2))
println("10 / 0 = ", safe_divide(10, 0))
println("20 / 4 = ", safe_divide(20, 4))

println(safe_divide(100, 5))`,
    ir: null
  },

  // ==================== ADVANCED ====================
  {
    name: "Sieve of Eratosthenes",
    code: `function sieve(n)
    is_prime = ones(n)
    is_prime[1] = 0  # 1 is not prime

    for i in 2:sqrt(n)
        if is_prime[i] == 1
            j = i * 2
            while j <= n
                is_prime[j] = 0
                j += i
            end
        end
    end

    count = 0
    for i in 1:n
        if is_prime[i] == 1
            count += 1
        end
    end
    count
end

@time count = sieve(100)
println("Primes up to 100: ", count)

@time count = sieve(1000)
println("Primes up to 1000: ", count)

println(count)`,
    ir: null
  },
  {
    name: "Sum of Primes",
    code: `function is_prime(n)
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

println("Sum of primes up to 100: ", sum_primes(100))
println("Sum of primes up to 1000: ", sum_primes(1000))

println(sum_primes(100))`,
    ir: null
  },
  {
    name: "Mandelbrot Set",
    code: `# Mandelbrot escape time algorithm
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

# Compute grid using broadcast (vectorized)
# xs' creates a row vector, ys is a column vector
# Broadcasting creates a 2D complex matrix C
function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    # Create 2D complex grid via broadcasting
    C = xs' .+ im .* ys

    # Apply escape function to all points at once
    # Ref(maxiter) prevents maxiter from being broadcast
    mandelbrot_escape.(C, Ref(maxiter))
end

# ASCII visualization
@time grid = mandelbrot_grid(50, 25, 50)
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

println(grid[12, 25])`,
    ir: null
  },
  {
    name: "Monte Carlo Integration",
    code: `function monte_carlo_integral(N)
    # Estimate integral of x^2 from 0 to 1
    sum = 0.0
    for i in 1:N
        x = rand()
        sum += x^2
    end
    sum / N  # Should be close to 1/3
end

@time result = monte_carlo_integral(100000)
println("Integral of x^2 from 0 to 1: ", result)
println("Expected: 0.333...")

println(result)`,
    ir: null
  },
  {
    name: "Dice Simulation",
    code: `
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

avg = simulate_dice(10000)
println("Average of 10000 dice rolls: ", avg)
println("Expected: 3.5")

println(avg)`,
    ir: null
  },
  {
    name: "Normal Distribution Histogram",
    code: `# Visualize normal distribution with histogram
n = 1000
samples = randn(n)

# Count samples in bins: <-2, [-2,-1), [-1,0), [0,1), [1,2), >=2
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

println("Normal Distribution Histogram (n=1000):")
println("  x < -2:      ", bins[1])
println(" -2 <= x < -1: ", bins[2])
println(" -1 <= x <  0: ", bins[3])
println("  0 <= x <  1: ", bins[4])
println("  1 <= x <  2: ", bins[5])
println("  x >= 2:      ", bins[6])

println(bins[3] + bins[4])`,
    ir: null
  },
  {
    name: "Newton's Method",
    code: `function newton_sqrt(x)
    guess = x / 2.0
    for i in 1:10
        guess = (guess + x / guess) / 2.0
    end
    guess
end

println("newton_sqrt(2) = ", newton_sqrt(2.0))
println("sqrt(2) = ", sqrt(2.0))

println("newton_sqrt(10) = ", newton_sqrt(10.0))
println("sqrt(10) = ", sqrt(10.0))

println(newton_sqrt(2.0))`,
    ir: null
  },
  {
    name: "Taylor Series e^x",
    code: `function exp_taylor(x, terms)
    # e^x ≈ 1 + x + x^2/2! + x^3/3! + ...
    result = 1.0
    term = 1.0
    for n in 1:terms
        term = term * x / n
        result += term
    end
    result
end

println("exp_taylor(1.0, 20) = ", exp_taylor(1.0, 20))
println("Expected e ≈ 2.71828...")

println("exp_taylor(2.0, 20) = ", exp_taylor(2.0, 20))

println(exp_taylor(1.0, 20))`,
    ir: null
  },
  {
    name: "Coprime Pi Estimation",
    code: `# Estimate π using coprime probability
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

@time println("N=100: π ≈ ", calc_pi(100))
@time println("N=500: π ≈ ", calc_pi(500))

println(calc_pi(100))`,
    ir: null
  },
  {
    name: "Higher-Order Functions",
    code: `# map, filter, reduce, sum
arr = [1, 2, 3, 4, 5]

# Square each element
function square(x)
    x * x
end
squared = map(square, arr)
println("squared: ", squared)

# Filter even numbers
function is_even(x)
    x % 2 == 0
end
evens = filter(is_even, arr)
println("evens: ", evens)

# Sum all elements (direct)
total = sum(arr)
println("sum(arr): ", total)

# Sum with transformation: sum(f, arr)
sum_of_squares = sum(square, arr)
println("sum(square, arr): ", sum_of_squares)

# Reduce with binary function
function add(a, b)
    a + b
end
reduced = reduce(add, arr)
println("reduce(add, arr): ", reduced)

sum_of_squares`,
    ir: null
  },
  {
    name: "Particle Simulation",
    code: `mutable struct Particle
    x::Float64
    y::Float64
    vx::Float64
    vy::Float64
end

function step!(p, dt)
    p.x = p.x + p.vx * dt
    p.y = p.y + p.vy * dt
end

# Create particle with initial position and velocity
particle = Particle(0.0, 0.0, 1.0, 0.5)
println("Initial: (", particle.x, ", ", particle.y, ")")

# Simulate 10 time steps
dt = 0.1
for t in 1:10
    step!(particle, dt)
    println("t=", t * dt, ": (", particle.x, ", ", particle.y, ")")
end

println(sqrt(particle.x^2 + particle.y^2))`,
    ir: null
  },
  {
    name: "MyGeometry Module",
    code: `module MyGeometry

using Statistics: mean

export distance
export Point

struct Point{T<:Real}
    x::T
    y::T
end

Base.:+(p::Point{T}, q::Point{T}) where T <: Real = Point{T}(p.x + q.x, p.y + q.y)
Base.:-(p::Point{T}, q::Point{T}) where T <: Real = Point{T}(p.x - q.x, p.y - q.y)

function distance(p::Point{T}, q::Point{T}) where T <: Real
    return sqrt((q.x - p.x)^2 + (q.y - p.y)^2)
end

function centroid(points::Vector{Point{T}}) where T <: Real
    x = mean([point.x for point in points])
    y = mean([point.y for point in points])
    Tnew = promote_type(typeof(x), typeof(y))
    return Point{Tnew}(x, y)
end

end #module

using .MyGeometry

p = Point(3, 4)
q = Point(0, 0)

@assert distance(p, q) == 5.0
@assert p isa Point{Int}
@assert typeof(p) === Point{Int}
@assert p + q == Point(3, 4)
@assert p - q == Point(3, 4)
@assert MyGeometry.centroid([Point(1, 2), Point(3, 4), Point(5, 6)]) == Point(3.0, 4.0)

println("All assertions passed!")
println(distance(p, q))`,
    ir: null
  },
  {
    name: "Complex Numbers",
    code: `# Complex number operations
z1 = complex(3.0, 4.0)
z2 = complex(1.0, 2.0)

println("z1 = ", z1)
println("z2 = ", z2)
println("z1 + z2 = ", z1 + z2)
println("z1 * z2 = ", z1 * z2)
println("|z1| = ", abs(z1))

abs(z1)`,
    ir: null
  },
  {
    name: "Meta.parse & eval",
    code: `# Meta.parse and eval - Metaprogramming in Julia
# Parse a string into an expression and evaluate it

# Parse a simple expression
expr = Meta.parse("1+1")
println("Expression: ", expr)
println("Head: ", expr.head)
println("Args: ", expr.args)
println("Result: ", eval(expr))

println("")

# Parse a more complex expression
expr2 = Meta.parse("2 * 3 + 4")
println("Expression: ", expr2)
println("Head: ", expr2.head)
println("Args: ", expr2.args)
println("Result: ", eval(expr2))

println("")

# Parse a function call
expr3 = Meta.parse("sqrt(16)")
println("Expression: ", expr3)
println("Head: ", expr3.head)
println("Args: ", expr3.args)
println("Result: ", eval(expr3))

println("")

# Expr variables persist correctly - can be reused
println("Verifying Expr persistence:")
println("eval(expr) = ", eval(expr))    # Still returns 2
println("eval(expr2) = ", eval(expr2))  # Still returns 10
println("eval(expr3) = ", eval(expr3))  # Still returns 4.0

eval(expr)`,
    ir: null
  },
  {
    name: "User-Defined Macros",
    code: `# User-Defined Macros - Create your own macros in Julia

# Simple macro that doubles an expression
macro twice(expr)
    quote
        $(esc(expr))
        $(esc(expr))
    end
end

println("=== @twice macro ===")
@twice println("Hello!")

println("")

# Macro that wraps code with before/after messages
macro debug(expr)
    quote
        println("[DEBUG] Executing...")
        result = $(esc(expr))
        println("[DEBUG] Done. Result: ", result)
        result
    end
end

println("=== @debug macro ===")
x = @debug 1 + 2 + 3
println("x = ", x)

println("")

# Macro that adds two expressions
macro add(a, b)
    quote
        $(esc(a)) + $(esc(b))
    end
end

println("=== @add macro ===")
result = @add 10 20
println("@add 10 20 = ", result)

println("")

# Macro that multiplies an expression by 2
macro double(expr)
    quote
        2 * $(esc(expr))
    end
end

println("=== @double macro ===")
println("@double 21 = ", @double 21)
println("@double 3 + 4 = ", @double 3 + 4)

result`,
    ir: null
  }
];
