# Test Bool ^ Bool returns Bool with correct semantics (Issue #1971)
# Julia defines ^(x::Bool, y::Bool) = x || !y

using Test

@testset "Bool ^ Bool returns Bool type" begin
    @test typeof(true ^ true) == Bool
    @test typeof(true ^ false) == Bool
    @test typeof(false ^ true) == Bool
    @test typeof(false ^ false) == Bool
end

@testset "Bool ^ Bool value correctness" begin
    # ^(x::Bool, y::Bool) = x || !y
    @test (true ^ true) == true     # true || !true = true || false = true
    @test (true ^ false) == true    # true || !false = true || true = true
    @test (false ^ true) == false   # false || !true = false || false = false
    @test (false ^ false) == true   # false || !false = false || true = true
end

true
