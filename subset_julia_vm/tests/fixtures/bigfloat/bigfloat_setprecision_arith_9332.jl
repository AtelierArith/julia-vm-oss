using Test

# Issue #9332: inside a `setprecision(BigFloat, p)` context, the *result* of a
# BigFloat binary/unary operation must carry precision `p` (upstream allocates
# every op result at the active default precision), not the default 256-bit.
# Verified against upstream julia 1.12.
#
# NOTE: the `@test`s are placed OUTSIDE the `setprecision(...) do ... end`
# blocks — sjulia does not yet lower a macro call inside a `do`-block closure
# body (Issue #9598), so we capture the precisions inside the block and assert
# them outside.

# Keep floor on a non-zero quotient so this fixture stays focused on the #9332
# arithmetic result-precision contract; zero-result precision is covered by
# bigfloat_zero_precision_9599.jl.
p64 = setprecision(BigFloat, 64) do
    a = BigFloat(7); b = BigFloat(3)
    (precision(a), precision(a + b), precision(a - b), precision(a * b),
     precision(a / b), precision(-a), precision(floor(a / b)), precision(sqrt(b)))
end

p128 = setprecision(BigFloat, 128) do
    x = BigFloat(2)
    (precision(x * x), precision(x + BigFloat(1)), precision(x + big(typemax(Int128))))
end

exact_integer_results = setprecision(BigFloat, 64) do
    # Issue #9603: mixed BigFloat/integer operations keep this integer exact
    # until the final BigFloat destination rounding.
    n = big(2)^64 + 1
    (
     BigFloat(1) + n,
     (-BigFloat(1))^n,
     BigFloat(big(2)^64 + 2),
     BigFloat(-1),
     )
end

exact_integer_matrix = setprecision(BigFloat, 64) do
    # Issue #9605: cover the exact-integer operand contract across the
    # representative BigFloat mixed-operation routes. BigInt is the upstream
    # MPFR path that keeps integers exact even when the integer is wider than
    # the active result precision.
    n = big(2)^64 + 1
    (
     BigFloat(1) + n == BigFloat(big(2)^64 + 2),
     n - BigFloat(3) == BigFloat(big(2)^64 - 2),
     BigFloat(3) * n == BigFloat(3 * (big(2)^64 + 1)),
     BigFloat(3) / n != BigFloat(3) / BigFloat(n),
     BigFloat(2)^64 < n,
     BigFloat(n) < n,
     (-BigFloat(1))^n == BigFloat(-1),
     precision(BigFloat(1) + n),
     precision(n - BigFloat(3)),
     precision(BigFloat(3) * n),
     precision(BigFloat(3) / n),
     precision((-BigFloat(1))^n),
     )
end

int64_low_precision_results = setprecision(BigFloat, 4) do
    # Upstream keeps an Int64 operand exact for the operation even though
    # `BigFloat(17)` itself rounds at the active precision.
    (
     BigFloat(1) + 17 == BigFloat(18),
     BigFloat(1) + 17 != BigFloat(17),
     )
end

pdef = precision(BigFloat(1) + BigFloat(1))

@testset "BigFloat arithmetic honors setprecision (Issue #9332)" begin
    @test all(==(64), p64)
    @test all(==(128), p128)
    # Issue #9600: avoid tuple range indexing in this fixture until supported.
    @test exact_integer_results[1] == exact_integer_results[3]
    @test exact_integer_results[2] == exact_integer_results[4]
    @test all(==(64), map(precision, exact_integer_results))
    @test all((exact_integer_matrix[1], exact_integer_matrix[2],
               exact_integer_matrix[3], exact_integer_matrix[4],
               exact_integer_matrix[5], exact_integer_matrix[6],
               exact_integer_matrix[7]))
    @test all(==(64), (exact_integer_matrix[8], exact_integer_matrix[9],
                       exact_integer_matrix[10], exact_integer_matrix[11],
                       exact_integer_matrix[12]))
    @test all(int64_low_precision_results)
    @test pdef == 256
end

true
