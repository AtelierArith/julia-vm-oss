using Test

@testset "function negation composes predicates" begin
    @test filter(!iseven, [1, 2, 3, 4]) == [1, 3]
    @test map(!isnothing, Any[1, nothing, 2]) == Bool[true, false, true]

    not_even = !iseven
    @test not_even(1) == true
    @test not_even(2) == false
    @test map(not_even, [1, 2, 3]) == Bool[true, false, true]

    @test ((!) ∘ iseven)(1) == true
    @test ((!) ∘ iseven)(2) == false
    @test map(!!iseven, [1, 2, 3]) == Bool[false, true, false]
end

true
