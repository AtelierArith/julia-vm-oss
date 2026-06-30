using Test

@testset "Memory abstract numeric constructor preserves boxed values (Issue #4239)" begin
    real_values = Memory{Real}(undef, 2)
    @test typeof(real_values) === Memory{Real}
    @test eltype(real_values) === Real
    real_values[1] = 1
    real_values[2] = 2.0
    @test real_values[1] == 1
    @test typeof(real_values[1]) === Int64
    @test real_values[2] == 2.0
    @test typeof(real_values[2]) === Float64

    integer_values = Memory{Integer}(undef, 2)
    @test typeof(integer_values) === Memory{Integer}
    @test eltype(integer_values) === Integer
    integer_values[1] = Int32(1)
    integer_values[2] = 2
    @test typeof(integer_values[1]) === Int32
    @test typeof(integer_values[2]) === Int64

    abstract_float_values = Memory{AbstractFloat}(undef, 2)
    @test typeof(abstract_float_values) === Memory{AbstractFloat}
    @test eltype(abstract_float_values) === AbstractFloat
    abstract_float_values[1] = Float32(1.5)
    abstract_float_values[2] = 2.5
    @test typeof(abstract_float_values[1]) === Float32
    @test typeof(abstract_float_values[2]) === Float64
end

true
