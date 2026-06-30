using Test

macro macro_pair_call_7639()
    esc(:(Dict(:a => 1)))
end

@testset "macro-expanded Pair calls lower as Pair expressions (Issue #7639)" begin
    d = @macro_pair_call_7639()
    @test d[:a] == 1
end

true
