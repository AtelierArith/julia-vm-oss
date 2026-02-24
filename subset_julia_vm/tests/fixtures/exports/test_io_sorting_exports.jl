# Test exported IO and sorting functions

using Test

@testset "IO and sorting exports" begin
    # IOBuffer - in-memory I/O buffer
    buf = IOBuffer()
    @test buf isa IOBuffer

    # IOContext - IO with properties
    ctx = IOContext(buf, :compact => true)
    @test ctx isa IOContext

    # sortperm! - in-place sort permutation
    arr = [3.0, 1.0, 2.0]
    perm = [1, 2, 3]
    sortperm!(perm, arr)
    @test perm == [2, 3, 1]
end

true
