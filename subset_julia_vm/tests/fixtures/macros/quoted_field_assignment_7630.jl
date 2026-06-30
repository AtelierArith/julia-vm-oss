using Test

macro quoted_field_assignment_7630(x, f, v)
    QuoteNode(:($x.$f = $v))
end

@testset "macro expansion can return quoted field assignment Expr (Issue #7630)" begin
    ex = @quoted_field_assignment_7630 obj field 3

    @test ex isa Expr
    @test ex.head == :(=)
    @test string(ex.args[1]) == "obj.field"
    @test ex.args[2] == 3
    @test string(ex) == "obj.field = 3"
end

true
