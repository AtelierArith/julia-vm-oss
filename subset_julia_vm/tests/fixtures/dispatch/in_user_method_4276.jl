using Test

import Base: in
import Base: ∈
import Base: ∉
import Base: ∋
import Base: ∌

in(x::Int64, ys::Vector{Int64}) = 4276
∈(x::Float64, ys::Vector{Float64}) = 4277
∉(x::Int64, ys::Vector{Int64}) = 4278
∋(ys::Vector{Int64}, x::Int64) = 4279
∌(ys::Vector{Int64}, x::Int64) = 4280

runtime_in_4276(x::Any, ys::Any) = in(x, ys)
runtime_in_alias_4276(x::Any, ys::Any) = x ∈ ys
runtime_notin_alias_4276(x::Any, ys::Any) = x ∉ ys
runtime_contains_alias_4276(ys::Any, x::Any) = ys ∋ x
runtime_notcontains_alias_4276(ys::Any, x::Any) = ys ∌ x

@testset "membership user methods before fallback (Issue #4276)" begin
    @test in(1, [1, 2]) == 4276
    @test runtime_in_4276(1, [1, 2]) == 4276

    any_values = Any[1, 2]
    @test in(1, any_values) == true
    @test in(3, any_values) == false
    @test runtime_in_4276(1, any_values) == true
    @test runtime_in_4276(3, any_values) == false

    any_float_values = Any[1.0, 2.0]
    @test (1.0 ∈ [1.0, 2.0]) == 4277
    @test runtime_in_alias_4276(1.0, [1.0, 2.0]) == 4277
    @test (1.0 ∈ any_float_values) == true
    @test runtime_in_alias_4276(3.0, any_float_values) == false

    @test (1 ∉ [1, 2]) == 4278
    @test runtime_notin_alias_4276(1, [1, 2]) == 4278
    @test (1 ∉ any_values) == false
    @test runtime_notin_alias_4276(3, any_values) == true

    @test ([1, 2] ∋ 1) == 4279
    @test runtime_contains_alias_4276([1, 2], 1) == 4279
    @test (any_values ∋ 1) == true
    @test runtime_contains_alias_4276(any_values, 3) == false

    @test ([1, 2] ∌ 1) == 4280
    @test runtime_notcontains_alias_4276([1, 2], 1) == 4280
    @test (any_values ∌ 1) == false
    @test runtime_notcontains_alias_4276(any_values, 3) == true
end

true
