using Test

# Issue #7641: quoting an assignment whose LHS is a vector/array pattern must
# preserve the LHS as `Expr(:vect, ...)`, and `print`/`string` of that Expr must
# render the Julia-code form (`[a, b]`) rather than the s-expr constructor form
# (`Expr(:vect, ...)`). The latter is only produced by `dump`/`Meta.show_sexpr`.
@testset "quoted vector LHS prints as Expr(:vect) (Issue #7641)" begin
    ex = :([a, b] = Dict(:a => 1, :b => 2))

    # LHS is an Expr(:vect, ...), not a Symbol.
    @test ex.args[1] isa Expr
    @test ex.args[1].head === :vect
    @test ex.args[1].args == [:a, :b]

    # print/string of the vector pattern renders the Julia-code form.
    @test sprint(print, ex.args[1]) == "[a, b]"
    @test string(ex.args[1]) == "[a, b]"
    @test "$(ex.args[1])" == "[a, b]"

    # The LHS pattern of a simpler quoted assignment also round-trips.
    asn = :([a, b] = c)
    @test asn.args[1] isa Expr
    @test sprint(print, asn) == "[a, b] = c"

    # A bare vector literal expression prints the same way.
    @test sprint(print, Expr(:vect, :x, :y, :z)) == "[x, y, z]"
    @test string(Expr(:vect, 1, 2, 3)) == "[1, 2, 3]"
    @test sprint(print, Expr(:vect)) == "[]"
end

true
