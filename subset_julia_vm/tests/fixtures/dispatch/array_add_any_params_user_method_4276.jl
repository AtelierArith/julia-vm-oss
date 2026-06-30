using Test

import Base: +

+(a::Vector{Int64}, b::Vector{Int64}) = [42]

array_add_any_params_4276(a::Any, b::Any) = a + b
array_add_untyped_params_4276(a, b) = a + b

function array_add_untyped_full_4276(a, b)
    a + b
end

@testset "array + Any params uses user method before fallback (Issue #4276)" begin
    @test array_add_any_params_4276([1, 2], [3, 4]) == [42]
    @test typeof(array_add_any_params_4276([1, 2], [3, 4])) === Vector{Int64}

    @test array_add_untyped_params_4276([1, 2], [3, 4]) == [42]
    @test typeof(array_add_untyped_params_4276([1, 2], [3, 4])) === Vector{Int64}

    @test array_add_untyped_full_4276([1, 2], [3, 4]) == [42]
    @test typeof(array_add_untyped_full_4276([1, 2], [3, 4])) === Vector{Int64}

    @test array_add_any_params_4276([1.0, 2.0], [3.0, 4.0]) == [4.0, 6.0]
    @test array_add_untyped_params_4276([1.0, 2.0], [3.0, 4.0]) == [4.0, 6.0]
end

true
