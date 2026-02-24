# Test displaysize function for getting terminal display dimensions
#
# displaysize() returns a tuple (rows, columns) representing the terminal size.
# For IOContext with :displaysize property, it returns the custom size.

using Test

@testset "displaysize" begin
    # Test default displaysize
    size = displaysize()
    @test size == (24, 80)
    @test size[1] == 24
    @test size[2] == 80

    # Test displaysize with IOBuffer
    io = IOBuffer()
    @test displaysize(io) == (24, 80)

    # Test displaysize with IOContext - custom size
    ctx = iocontext(io, :displaysize => (50, 120))
    custom_size = displaysize(ctx)
    @test custom_size == (50, 120)
    @test custom_size[1] == 50
    @test custom_size[2] == 120

    # Test displaysize with IOContext - no displaysize property
    ctx2 = iocontext(io, :compact => true)
    @test displaysize(ctx2) == (24, 80)
end

true
