using Test

@testset "type value equality via mutual subtyping (Issue #5921)" begin
    @test Tuple == Tuple{Vararg{Any}}
    @test Tuple{Vararg{Any}} == Tuple
    @test Tuple === Tuple{Vararg{Any}}
    @test Tuple{Vararg{Any}} === Tuple
    @test !(Tuple == Tuple{Any})
    @test !(Tuple === Tuple{Any})
end

true
