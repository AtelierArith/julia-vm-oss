# Test NamedTuple getfield type inference (Issue #1638)
# getfield((a=1, b=2.0), :b) should infer as Float64

using Test

@testset "NamedTuple getfield inference" begin
    # Basic NamedTuple field access
    nt1 = (a=1, b=2.0)
    @test nt1.a == 1
    @test typeof(nt1.a) == Int64
    @test nt1.b == 2.0
    @test typeof(nt1.b) == Float64

    # Direct NamedTuple literal field access
    @test (x=10, y="hello").x == 10
    @test typeof((x=10, y="hello").x) == Int64
    @test (x=10, y="hello").y == "hello"
    @test typeof((x=10, y="hello").y) == String

    # Mixed type NamedTuple
    mixed = (flag=true, count=42, value=3.14, name="test")
    @test mixed.flag == true
    @test typeof(mixed.flag) == Bool
    @test mixed.count == 42
    @test typeof(mixed.count) == Int64
    @test mixed.value == 3.14
    @test typeof(mixed.value) == Float64
    @test mixed.name == "test"
    @test typeof(mixed.name) == String

    # getfield function call
    @test getfield(nt1, :a) == 1
    @test typeof(getfield(nt1, :a)) == Int64
    @test getfield(nt1, :b) == 2.0
    @test typeof(getfield(nt1, :b)) == Float64
end

true
