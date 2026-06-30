using Test

function narrow_int_local_roundtrip_5081()
    x = Int8(7)
    return x
end

function narrow_int_local_reassign_5081()
    x = UInt8(9)
    x = 42
    return x
end

@testset "narrow integer local carrier consolidation (Issue #5081)" begin
    @test narrow_int_local_roundtrip_5081() === Int8(7)
    @test narrow_int_local_reassign_5081() == 42
end

true
