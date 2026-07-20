using Test

@testset "dotted parametric constructor callee (Issue #10475)" begin
    xs = [[1], [2, 3]]
    ys = Vector{Int64}.(xs)
    @test ys == xs
    @test ys[1] isa Vector{Int64}
    # Outer broadcast result eltype precision is tracked separately by #10535.

    ctor = (Vector{Float64},)
    zs = ctor[1].(xs)
    @test zs == [[1.0], [2.0, 3.0]]
end

true
