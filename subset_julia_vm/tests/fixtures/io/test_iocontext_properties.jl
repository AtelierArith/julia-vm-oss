# Test IOContext property access using ioget and iohaskey

using Test

@testset "IOContext property access" begin
    # Test single property
    ctx1 = iocontext(stdout, :compact => true)
    @test ioget(ctx1, :compact, false) == true
    @test ioget(ctx1, :limit, false) == false
    @test iohaskey(ctx1, :compact) == true
    @test iohaskey(ctx1, :limit) == false

    # Test multiple properties
    ctx2 = iocontext(stdout, :compact => true, :limit => true)
    @test ioget(ctx2, :compact, false) == true
    @test ioget(ctx2, :limit, false) == true
    @test ioget(ctx2, :color, false) == false
    @test iohaskey(ctx2, :compact) == true
    @test iohaskey(ctx2, :limit) == true
    @test iohaskey(ctx2, :color) == false

    # Test displaysize
    ctx3 = iocontext(stdout, :displaysize => (40, 100))
    ds = ioget(ctx3, :displaysize, (24, 80))
    @test ds == (40, 100)

    # Test empty context
    ctx_empty = iocontext(stdout)
    @test ioget(ctx_empty, :compact, false) == false
    @test iohaskey(ctx_empty, :compact) == false

    # Test context with non-boolean values
    ctx4 = iocontext(stdout, :typeinfo => Int64)
    ti = ioget(ctx4, :typeinfo, Any)
    @test ti == Int64
end

true
