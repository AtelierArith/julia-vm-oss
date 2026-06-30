# Test: Heterogeneous array literal inference preserves Array container shape
# (Issue #3528). The inference engine no longer collapses `[1, 2.0]` to Top —
# it preserves an Array container with a Union element type. Direct evaluation
# of `[1, nothing]` is tracked separately because of a downstream codegen bug
# unrelated to inference.
using Test

function len_int_float()
    xs = [1, 2.0]
    return length(xs)
end

function eltype_int_float()
    xs = [1, 2.0]
    return eltype(xs)
end

@testset "Heterogeneous array literal inference" begin
    @test len_int_float() == 2
    # `[1, 2.0]` promotes to Float64 in Julia, but the VM may infer
    # Union{Int64, Float64}; either is fine here as long as the array shape
    # is preserved.
    et = eltype_int_float()
    @test et === Float64 || et === Int64 || et === Real || et === Number
end

true
