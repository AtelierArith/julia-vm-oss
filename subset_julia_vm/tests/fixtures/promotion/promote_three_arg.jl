using Test

# Test 3-argument promote and promote_type (Issue #2248).
# Previously failed with "Unbound type parameter: T1" because
# is_type_variable_name only recognized single-letter names (T, S)
# but not multi-character names like T1, T2, T3.

@testset "3-arg promote_type" begin
    @test promote_type(Float32, Int64, Bool) == Float32
    @test promote_type(Float64, Int64, Bool) == Float64
    @test promote_type(Int64, Int32, Int16) == Int64
end

@testset "3-arg promote" begin
    r = promote(Float32(1.5), Int64(2), true)
    @test typeof(r[1]) == Float32
    @test typeof(r[2]) == Float32
    @test typeof(r[3]) == Float32
    @test r[1] == Float32(1.5)
    @test r[2] == Float32(2.0)
    @test r[3] == Float32(1.0)
end

@testset "4-arg promote_type" begin
    @test promote_type(Float32, Int64, Bool, Int32) == Float32
end

true
