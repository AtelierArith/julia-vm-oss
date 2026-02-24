using Test

# Test get() and haskey() on IOContext (Issue #3152)
@testset "IOContext get and haskey" begin
    buf = IOBuffer()
    ctx = iocontext(buf, :compact => true)

    # get() returns the value for a set key
    @test get(ctx, :compact, false) == true

    # get() returns default for a missing key
    @test get(ctx, :limit, false) == false

    # haskey() returns true for a set key
    @test haskey(ctx, :compact) == true

    # haskey() returns false for a missing key
    @test haskey(ctx, :limit) == false

    # Multiple properties
    buf2 = IOBuffer()
    ctx2 = iocontext(buf2, :compact => true, :limit => true)
    @test get(ctx2, :compact, false) == true
    @test get(ctx2, :limit, false) == true
    @test haskey(ctx2, :compact) == true
    @test haskey(ctx2, :limit) == true
    @test get(ctx2, :color, false) == false
    @test haskey(ctx2, :color) == false
end

true
