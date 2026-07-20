using Test

function nothing_local_roundtrip_5081(flag)
    x = nothing
    if flag
        return x === nothing
    end
    y = x
    return y
end

function nothing_local_reassign_5081()
    x = nothing
    x = 42
    return x
end

@testset "nothing local carrier consolidation (Issue #5081)" begin
    @test nothing_local_roundtrip_5081(true) === true
    @test nothing_local_roundtrip_5081(false) === nothing
    @test nothing_local_reassign_5081() == 42
end

true
