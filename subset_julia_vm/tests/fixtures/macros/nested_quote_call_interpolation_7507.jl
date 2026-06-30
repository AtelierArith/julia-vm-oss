using Test

struct TypeBind
    name::Symbol
    ts::Set{Any}
end

@testset "nested quote interpolation in call arguments (Issue #7507)" begin
    name = :x
    ts = [:call]
    ex = Expr(:$, :($TypeBind($(Expr(:quote, name)), Set{Any}([$(ts...)]))))
    inner = ex.args[1]
    set_call = inner.args[3]
    vect = set_call.args[2]

    @test ex isa Expr
    @test ex.head == :$
    @test inner isa Expr
    @test inner.head == :call
    @test inner.args[2] == Expr(:quote, :x)
    @test set_call isa Expr
    @test set_call.head == :call
    @test vect isa Expr
    @test vect.head == :vect
    @test vect.args[1] == :call
end

true
