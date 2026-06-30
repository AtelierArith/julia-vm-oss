using Test

@testset "Bool div preserves Bool result type (Issue #6486)" begin
    @test typeof(div(false, true)) === Bool
    @test div(false, true) === false
    @test typeof(div(true, true)) === Bool
    @test div(true, true) === true

    @test_throws DivideError div(false, false)
    @test_throws DivideError div(true, false)
end

true
