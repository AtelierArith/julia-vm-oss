using Test

module BareCallLocal10771
    inner10771(x) = x * 2
    driver10771(n) = inner10771(n)
end

inner10771(x) = x * 1000

@testset "bare calls inside a module prefer the module-local function (Issue #10771)" begin
    @test BareCallLocal10771.driver10771(4) == 8
    @test inner10771(4) == 4000
end

true
