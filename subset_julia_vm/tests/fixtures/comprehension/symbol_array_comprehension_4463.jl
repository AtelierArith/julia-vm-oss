# Array comprehensions preserve Symbol element values (Issue #4463)

using Test

@testset "Symbol array comprehension does not coerce through Float64 (Issue #4463)" begin
    syms = [:a, :b, :c]
    out = [s for s in syms]
    @test out == syms
    @test typeof(out) == Vector{Symbol}
end

true
