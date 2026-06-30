using Test
using MacroTools

ex7539 = :(f(1, "two"))
matched7539 = @capture(ex7539, f_(args__))

@testset "MacroTools @capture expansion quotes runtime Any payloads (Issue #7539)" begin
    @test matched7539
    @test f == :f
    @test length(args) == 2
end

true
