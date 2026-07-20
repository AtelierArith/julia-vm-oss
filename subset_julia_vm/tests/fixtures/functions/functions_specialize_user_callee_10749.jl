using Test

# Issue #10749: the runtime specializer can compile a call to ANOTHER
# user-defined function. Before this, any callee outside the specializer's
# builtin whitelist aborted the caller's whole specialization
# (`Unsupported("Function call '<f>' not yet supported for specialization")`).
#
# These are semantic (output-parity) tests: the specializer is a pure
# optimization, so every case below must produce the SAME answer as upstream
# Julia whether or not it specializes. What they guard is that the new
# cross-function call path (and its recursion/return-type/error fallbacks)
# never changes an answer.

# --- untyped caller -> untyped callee, concrete args -------------------------
add_one(x) = x + 1

function sum_add_one(n)
    total = 0
    for i in 1:n
        total += add_one(i)
    end
    total
end

# Float callee reached from an integer-typed caller.
scale(x, k) = x * k

function sum_scaled(n)
    total = 0.0
    for i in 1:n
        total += scale(Float64(i), 2.5)
    end
    total
end

# Complex-valued callee (the ComplexF64 SROA convention: a specialized body
# keeps Complex values split into (re, im) F64 slots, so a ComplexF64 RESULT
# coming back from a callee frame must be unboxed at the call site).
make_c(re, im_part) = re + im_part * im

function complex_norm_sum(n)
    total = 0.0
    for i in 1:n
        z = make_c(Float64(i), -Float64(i))
        total += abs2(z)
    end
    total
end

# --- recursion: must not hang or blow the specializer ------------------------
fact(n) = n <= 1 ? 1 : n * fact(n - 1)

fib(n) = n < 2 ? n : fib(n - 1) + fib(n - 2)

# --- mutual recursion cycle --------------------------------------------------
is_even_rec(n) = n == 0 ? true : is_odd_rec(n - 1)
is_odd_rec(n) = n == 0 ? false : is_even_rec(n - 1)

# --- callee whose return type is NOT statically resolvable -------------------
# The branch types differ (Int64 vs String), so no single concrete return type
# exists; the caller must fall back cleanly rather than assume one.
function maybe_string(x)
    if x > 0
        return x * 2
    else
        return "neg"
    end
end

function describe(x)
    r = maybe_string(x)
    string(r)
end

# --- heterogeneous-return callee (codex review of #10749) --------------------
# `maybe_string` above has TWO return sites with different types. The
# specializer's reported return type is last-write-wins, so a caller must NOT
# propagate it (that would type downstream instructions for a value the callee
# does not always produce). `het_sum` pins the Int64 branch's arithmetic.
function het_sum(n)
    acc = 0
    for i in 1:n
        acc += maybe_string(i)
    end
    acc
end

# --- callee that throws ------------------------------------------------------
function checked_div(a, b)
    if b == 0
        error("division by zero")
    end
    a / b
end

function safe_div(a, b)
    try
        checked_div(a, b)
    catch e
        -1.0
    end
end

@testset "cross-function runtime specialization (Issue #10749)" begin
    # untyped caller calling untyped callee
    @test sum_add_one(10) == 65
    @test sum_add_one(1) == 2
    @test sum_add_one(0) == 0
    # Repeat: the second call runs the INSTALLED specialization.
    @test sum_add_one(10) == 65
    @test sum_scaled(4) ≈ 25.0
    @test sum_scaled(4) ≈ 25.0

    # ComplexF64-returning callee
    @test make_c(3.0, 4.0) == 3.0 + 4.0im
    @test complex_norm_sum(3) ≈ 28.0
    @test complex_norm_sum(3) ≈ 28.0

    # direct recursion terminates
    @test fact(10) == 3628800
    @test fact(1) == 1
    @test fib(15) == 610
    @test fib(15) == 610

    # mutual recursion cycle terminates
    @test is_even_rec(10) == true
    @test is_odd_rec(10) == false
    @test is_even_rec(7) == false
    @test is_odd_rec(7) == true

    # unresolvable callee return type falls back correctly
    @test describe(5) == "10"
    @test describe(-1) == "neg"
    @test describe(5) == "10"
    @test het_sum(4) == 20
    @test het_sum(4) == 20

    # throwing callee: error propagation and try/catch
    @test safe_div(10.0, 2.0) ≈ 5.0
    @test safe_div(10.0, 0.0) == -1.0
    @test safe_div(10.0, 0.0) == -1.0
    @test_throws ErrorException checked_div(1.0, 0.0)
end

true
