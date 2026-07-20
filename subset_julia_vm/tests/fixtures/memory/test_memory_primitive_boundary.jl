using Test

function dynamic_memory_length(n)
    Memory{Int64}(undef, n)
end

@testset "Memory primitive boundary avoids Array compatibility paths" begin
    a = Memory{Int64}(undef, 3)
    b = Memory{Int64}(undef, 3)

    for i in 1:3
        a[i] = i
        b[i] = i
    end

    @test a == b
    b[2] = 99
    @test a != b

    # Issue #3976: reflection/collection builtins should inspect Memory
    # directly instead of constructing a temporary Array wrapper.
    @test sizeof(a) == 24
    u = Memory{UInt8}(undef, 4)
    @test sizeof(u) == 4
    @test 1 in a
    @test !(99 in a)

    @test_throws ArgumentError dynamic_memory_length(-1)
end

true
