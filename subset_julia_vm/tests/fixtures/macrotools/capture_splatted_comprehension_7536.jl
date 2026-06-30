using Test
using MacroTools

match_ex_7536 = :(f(1, "two"))
matched_7536 = @capture(match_ex_7536, f(cap_a_7536_, cap_b_7536_))

miss_ex_7536 = :(g(1))
@capture(miss_ex_7536, f(miss_a_7536_, miss_b_7536_))

@testset "MacroTools @capture splats generated binding assignments (Issue #7536)" begin
    @test matched_7536
    @test cap_a_7536 == 1
    @test cap_b_7536 == "two"

    @test miss_a_7536 === nothing
    @test miss_b_7536 === nothing
end

true
