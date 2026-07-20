using Test

module ConstShadowsBase10234
    const log = Int[]
    record() = (push!(log, 1); length(log))
end

@testset "module const shadows Base function names inside method bodies (Issue #10234)" begin
    @test ConstShadowsBase10234.record() == 1
    @test ConstShadowsBase10234.record() == 2
    @test length(ConstShadowsBase10234.log) == 2
end

true
