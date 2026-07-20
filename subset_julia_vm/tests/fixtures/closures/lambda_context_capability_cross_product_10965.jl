using Test

# Issue #10965: a static parametric type expression (`Tuple{Int64}`,
# `Complex{Float64}`) in a function body must NOT change closure
# representation or capture analysis. The first #10948 fix routed any body
# containing a parametric type expression into the nested-closure lowering
# mode and broke exactly this cross product.

# Assigned closure captured alongside an unrelated static parametric type.
function cross_assigned_10965(n)
    T = Tuple{Int64}
    f = x -> x + n
    (T, f(10))
end

# Inline closure passed to a HOF next to a static parametric type.
function cross_inline_10965(v)
    C = Complex{Float64}
    total = sum(map(x -> 2x, v))
    (C, total)
end

# Nested named closure capturing an outer local, with a static parametric
# type in the same body.
function cross_nested_10965(a)
    T = Tuple{Int64}
    function inner(b)
        a + b
    end
    (T, inner(5))
end

# Closure over a mutated capture plus a static parametric type expression.
function cross_mutating_10965()
    acc = 0
    bump = () -> (acc += 1)
    bump()
    bump()
    (Complex{Float64}, acc)
end

# Positive control: the builtin-spelled value-base shadow of Issue #10948
# must keep working while the unrelated cases above stay on the ordinary
# closure paths.
function cross_value_base_10965(Vector::Type, n)
    f = x -> x + n
    (Vector{Int64}, f(1))
end

@testset "static parametric types do not disturb closures (Issue #10965)" begin
    T1, r1 = cross_assigned_10965(4)
    @test T1 === Tuple{Int64}
    @test r1 == 14

    C2, r2 = cross_inline_10965([1, 2, 3])
    @test C2 === Complex{Float64}
    @test r2 == 12

    T3, r3 = cross_nested_10965(7)
    @test T3 === Tuple{Int64}
    @test r3 == 12

    C4, r4 = cross_mutating_10965()
    @test C4 === Complex{Float64}
    @test r4 == 2

    V5, r5 = cross_value_base_10965(Set, 41)
    @test V5 === Set{Int64}
    @test r5 == 42
end

true
