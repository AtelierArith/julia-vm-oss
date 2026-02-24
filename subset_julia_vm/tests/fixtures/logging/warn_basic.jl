# Test @warn macro - basic usage

using Test

@testset "@warn basic" begin
    @warn "This is a warning"
    @warn "Deprecated function - use bar() instead"

    @test true
end

true
