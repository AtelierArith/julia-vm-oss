using Test

@testset "Memory copyto!" begin
    src = Memory{Int64}(undef, 3)
    src[1] = 10
    src[2] = 20
    src[3] = 30

    dest = Memory{Int64}(undef, 5)
    result = copyto!(dest, src)
    @test result === dest
    @test dest[1] == 10
    @test dest[2] == 20
    @test dest[3] == 30

    offset_dest = Memory{Int64}(undef, 5)
    copyto!(offset_dest, 2, src, 1, 3)
    @test offset_dest[1] == 0
    @test offset_dest[2] == 10
    @test offset_dest[3] == 20
    @test offset_dest[4] == 30
    @test offset_dest[5] == 0

    overlap_forward = Memory{Int64}(undef, 5)
    for i in 1:5
        overlap_forward[i] = i
    end
    copyto!(overlap_forward, 2, overlap_forward, 1, 3)
    @test overlap_forward[1] == 1
    @test overlap_forward[2] == 1
    @test overlap_forward[3] == 2
    @test overlap_forward[4] == 3
    @test overlap_forward[5] == 5

    overlap_backward = Memory{Int64}(undef, 5)
    for i in 1:5
        overlap_backward[i] = i
    end
    copyto!(overlap_backward, 1, overlap_backward, 3, 3)
    @test overlap_backward[1] == 3
    @test overlap_backward[2] == 4
    @test overlap_backward[3] == 5
    @test overlap_backward[4] == 4
    @test overlap_backward[5] == 5

    zero_count = copyto!(dest, 10, src, 10, 0)
    @test zero_count === dest

    @test_throws ArgumentError copyto!(dest, 1, src, 1, -1)
    @test_throws BoundsError copyto!(dest, 0, src, 1, 1)
end

true
