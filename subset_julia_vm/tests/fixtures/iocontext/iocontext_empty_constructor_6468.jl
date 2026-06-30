using Test

@testset "IOContext empty constructor (Issue #6468)" begin
    buf = IOBuffer()
    ctx = IOContext(buf)

    @test get(ctx, :compact, false) == false
    @test haskey(ctx, :compact) == false

    configured = IOContext(buf, :compact => true)
    same = IOContext(configured)

    @test same === configured
    @test get(same, :compact, false) == true
end

true
