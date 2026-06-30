using Test

@testset "quote construction supports where expressions (Issues #7553, #7714)" begin
    ex = :(x where {T})

    @test ex isa Expr
    @test ex.head == :where
    @test length(ex.args) == 2
    @test ex.args[1] == :x
    @test ex.args[2] == :T

    bare = :(f(a::T) where T)
    @test bare isa Expr
    @test bare.head == :where
    @test length(bare.args) == 2
    @test bare.args[1] == :(f(a::T))
    @test bare.args[2] == :T

    braced = :(f(a::T) where {T})
    @test braced isa Expr
    @test braced.head == :where
    @test length(braced.args) == 2
    @test braced.args[1] == :(f(a::T))
    @test braced.args[2] == :T
end

true
