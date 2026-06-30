# Regression guard for Issue #6873 (found while profiling Issue #6846).
#
# Appending to an `Array{T}` wrapper now grows the backing `Memory` in place via
# the underlying Vec's amortized (geometric) growth, instead of reallocating an
# exact-size `Memory` and copying every prior element on each push — which made
# comprehensions and `push!` loops O(n^2). The in-place growth must preserve
# every element, its order, and its type across the (now internal) Vec
# reallocations that happen as the buffer doubles. This fixture stresses growth
# large enough to cross several reallocation boundaries and checks the result is
# byte-for-byte what upstream Julia 1.12 produces.

using Test

struct Pt
    x::Int
    y::Int
end

@testset "amortized array growth correctness (Issue #6873)" begin
    # --- large typed comprehension (Float64) ---
    n = 1000
    zf = Float64[Float64(i) for i in 1:n]
    @test length(zf) == n
    @test zf[1] == 1.0
    @test zf[500] == 500.0
    @test zf[n] == Float64(n)
    @test sum(zf) == n * (n + 1) / 2

    # --- large untyped comprehension (Int body) ---
    zi = [2 * i for i in 1:n]
    @test length(zi) == n
    @test zi[1] == 2
    @test zi[n] == 2 * n
    @test sum(zi) == n * (n + 1)

    # --- 2D comprehension (the surface-plot shape) ---
    m = 40
    z2 = Float64[Float64(i + 100 * j) for j in 1:m, i in 1:m]
    @test size(z2) == (m, m)
    @test z2[1, 1] == 101.0
    @test z2[m, m] == Float64(m + 100 * m)
    @test z2[3, 7] == Float64(7 + 100 * 3)

    # --- push! loop builds the same vector, in order ---
    a = Float64[]
    for i in 1:n
        push!(a, Float64(i * i))
    end
    @test length(a) == n
    @test a[1] == 1.0
    @test a[2] == 4.0
    @test a[n] == Float64(n * n)
    @test a[123] == Float64(123 * 123)

    # --- push! across element types preserves value + type ---
    ac = ComplexF64[]
    for i in 1:50
        push!(ac, Complex(Float64(i), Float64(-i)))
    end
    @test length(ac) == 50
    @test ac[1] == 1.0 - 1.0im
    @test ac[50] == 50.0 - 50.0im
    @test eltype(ac) == ComplexF64

    ap = Pt[]
    for i in 1:30
        push!(ap, Pt(i, 2 * i))
    end
    @test length(ap) == 30
    @test ap[1] == Pt(1, 2)
    @test ap[30] == Pt(30, 60)

    astrs = String[]
    for i in 1:20
        push!(astrs, string("v", i))
    end
    @test astrs[1] == "v1"
    @test astrs[20] == "v20"

    aany = []
    push!(aany, 1)
    push!(aany, "two")
    push!(aany, 3.0)
    push!(aany, Pt(4, 5))
    @test length(aany) == 4
    @test aany[2] == "two"
    @test aany[4] == Pt(4, 5)

    # --- mixed push!/index-mutate/push! must not corrupt across reallocation ---
    b = Int[]
    for i in 1:100
        push!(b, i)
    end
    b[1] = -1
    b[50] = -50
    for i in 101:200
        push!(b, i)
    end
    @test length(b) == 200
    @test b[1] == -1
    @test b[50] == -50
    @test b[100] == 100
    @test b[200] == 200

    # --- aliasing: `c = d` shares; push!(d, ...) is visible through c ---
    d = [10, 20, 30]
    c = d
    push!(d, 40)
    @test length(c) == 4
    @test c[4] == 40
end

true
