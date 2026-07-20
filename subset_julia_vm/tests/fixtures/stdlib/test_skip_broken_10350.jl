# Test.@test_skip records Broken WITHOUT evaluating the expression
# (Issue #10350; implemented by the unified @test-family recording harness,
# Issue #10273 / PR #10367). This fixture pins the contract:
#   - the skipped expression is never evaluated (error() must not throw)
#   - the record is Broken (never fails the run)
#   - @test_broken evaluates and records Broken on failure

using Test

evaluated = Ref(0)
function would_boom()
    evaluated[] += 1
    error("boom")
end

@testset "skip outer" begin
    @test_skip would_boom()
    @test_skip 1 == 2
    @test evaluated[] == 0
    @test_broken 1 == 2
    @test true
end

println("after")
true
