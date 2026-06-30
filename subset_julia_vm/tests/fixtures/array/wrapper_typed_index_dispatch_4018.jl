using Test

function wrapper_typed_read_4018(a::Array{Int64})
    a[2, 1] + a[1, 2]
end

function wrapper_typed_write_4018!(a::Array{Int64})
    a[2, 2] = 44
    a[2, 2]
end

@testset "Array wrapper typed indexing dispatch (Issue #4018)" begin
    mem = Memory{Int64}(undef, 4)
    for i in 1:4
        mem[i] = i
    end

    a = Base.wrap(Array, mem, (2, 2))
    @test wrapper_typed_read_4018(a) == 5
    @test wrapper_typed_write_4018!(a) == 44
    @test mem[4] == 44
end

true
