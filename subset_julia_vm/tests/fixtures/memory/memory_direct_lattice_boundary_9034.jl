# Issue #9034: direct Memory values are runtime-supported, but sjulia currently
# widens direct user-code Memory inference to Any because the compile lattice has
# no ConcreteType::Memory carrier. Upstream Julia 1.12.6 infers Memory{Int64} /
# Int64 for these helpers; sjulia pins the wider inference in Rust dump-bytecode
# tests while this parity fixture keeps runtime behavior unchanged.

using Test

function memory_make_9034()
    m = Memory{Int64}(undef, 2)
    m[1] = 7
    m[2] = 11
    return m
end

function memory_read_sum_9034(m::Memory{Int64})
    return m[1] + m[2]
end

function memory_roundtrip_9034()
    m = memory_make_9034()
    return typeof(m) == Memory{Int64} &&
        eltype(m) == Int64 &&
        length(m) == 2 &&
        m[1] == 7 &&
        m[2] == 11 &&
        memory_read_sum_9034(m) == 18
end

@testset "direct Memory runtime with documented lattice boundary (#9034)" begin
    m = memory_make_9034()
    @test typeof(m) == Memory{Int64}
    @test eltype(m) == Int64
    @test memory_read_sum_9034(m) == 18
    @test memory_roundtrip_9034()
end

memory_roundtrip_9034()
