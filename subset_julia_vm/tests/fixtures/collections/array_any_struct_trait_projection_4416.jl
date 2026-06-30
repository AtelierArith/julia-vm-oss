using Test

struct ArrayAnyStructTraitBox4416
    x::Int64
end

@testset "Any array trait projection preserves declared element type (Issue #4416)" begin
    values = Any[ArrayAnyStructTraitBox4416(1)]

    @test eltype(values) === Any
    @test valtype(values) === Any
    @test typeof(values) === Vector{Any}
end

true
