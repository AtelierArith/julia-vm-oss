# Test @error macro (logging, not exception)
# Note: @error is for logging errors, different from error() which throws

using Test

@testset "@error logging basic" begin
    @error "Something went wrong"
    @error "Failed operation - please check"

    @test true
end

true
