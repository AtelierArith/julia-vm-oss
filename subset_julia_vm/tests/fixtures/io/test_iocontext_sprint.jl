# Test IOContext comprehensive functionality
# Verifies IOContext creation, property access, and usage patterns

using Test

@testset "IOContext comprehensive" begin
    # Test 1: IOContext creation with single property
    ctx1 = iocontext(stdout, :compact => true)
    @test ioget(ctx1, :compact, false) == true
    @test iohaskey(ctx1, :compact) == true

    # Test 2: IOContext creation with multiple properties
    ctx2 = iocontext(stdout, :compact => true, :limit => true)
    @test ioget(ctx2, :compact, false) == true
    @test ioget(ctx2, :limit, false) == true

    # Test 3: IOContext property default values
    ctx3 = iocontext(stdout)
    @test ioget(ctx3, :compact, false) == false
    @test ioget(ctx3, :nonexistent, 42) == 42

    # Test 4: IOContext with displaysize tuple
    ctx4 = iocontext(stdout, :displaysize => (25, 80))
    ds = ioget(ctx4, :displaysize, (24, 80))
    @test ds == (25, 80)

    # Test 5: IOContext with IOBuffer
    io = IOBuffer()
    ctx5 = iocontext(io, :color => true)
    @test ioget(ctx5, :color, false) == true

    # Test 6: IOContext haskey for missing keys
    ctx6 = iocontext(stdout, :limit => true)
    @test iohaskey(ctx6, :limit) == true
    @test iohaskey(ctx6, :compact) == false
    @test iohaskey(ctx6, :color) == false

    # Test 7: IOContext with type value
    ctx7 = iocontext(stdout, :typeinfo => Int64)
    ti = ioget(ctx7, :typeinfo, Any)
    @test ti == Int64

    # Test 8: Multiple property access
    ctx8 = iocontext(stdout, :compact => true, :limit => true, :color => false)
    @test ioget(ctx8, :compact, false) == true
    @test ioget(ctx8, :limit, false) == true
    @test ioget(ctx8, :color, true) == false
end

true
