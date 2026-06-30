using Test

@testset "IOContext direct pair constructor (Issue #6409)" begin
    buf = IOBuffer()
    ctx = IOContext(buf, :compact => true)

    @test get(ctx, :compact, false) == true
    @test get(ctx, :limit, false) == false
    @test haskey(ctx, :compact) == true
    @test haskey(ctx, :limit) == false

    ctx2 = IOContext(buf, :compact => true, :limit => true)
    @test get(ctx2, :compact, false) == true
    @test get(ctx2, :limit, false) == true
    @test haskey(ctx2, :compact) == true
    @test haskey(ctx2, :limit) == true
end

true
