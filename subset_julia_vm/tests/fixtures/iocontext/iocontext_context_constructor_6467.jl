using Test

@testset "IOContext context constructor (Issue #6467)" begin
    source_buf = IOBuffer()
    source_ctx = IOContext(source_buf, :compact => true, :limit => true)

    same_io_ctx = IOContext(source_buf, source_ctx)
    @test get(same_io_ctx, :compact, false) == true
    @test get(same_io_ctx, :limit, false) == true

    target_buf = IOBuffer()
    target_ctx = IOContext(target_buf, source_ctx)
    @test get(target_ctx, :compact, false) == true
    @test get(target_ctx, :limit, false) == true
    @test haskey(target_ctx, :color) == false
end

true
