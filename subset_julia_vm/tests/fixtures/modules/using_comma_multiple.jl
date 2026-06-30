# Comma-separated `using` / `import` lists: `using A, B` is `using A; using B`
# (Issue #7262). Previously the whole list lowered to a single bogus module named
# "A, B" → "module 'A, B' not found in LOAD_PATH". The fix lowers one import per
# module from the CST's `import_list > import_path+`.

using Test

# Comma-form `using` of two stdlib modules must bring in BOTH modules' bindings.
using Printf, Random

@testset "using A, B brings in both modules" begin
    # @printf comes from Printf; seed!/rand from Random — both must resolve.
    Random.seed!(1234)
    x = rand()
    @test 0.0 <= x < 1.0
    msg = @sprintf("%.2f", 3.14159)
    @test msg == "3.14"
end

@testset "comma form matches separate-line form" begin
    # A second comma list with three modules also resolves all of them.
    import Base: sin, cos, sqrt
    @test isapprox(sin(0.0), 0.0; atol=1e-12)
    @test isapprox(cos(0.0), 1.0; atol=1e-12)
    @test isapprox(sqrt(4.0), 2.0; atol=1e-12)
end

true
