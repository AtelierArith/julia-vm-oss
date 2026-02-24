# Test @debug macro - basic usage

using Test

@testset "@debug basic" begin
    @debug "Debug information"
    @debug "More debug info"

    @test true
end

true
