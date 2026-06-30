using Test

@testset "quoted interpolated field expressions (Issue #7523)" begin
    x = :obj
    f = :field
    v = :val

    plus_eq = :($x.$f += $v)
    assign = :($x.$f = $v)
    plus_target = plus_eq.args[1]
    assign_target = assign.args[1]

    @test plus_eq isa Expr
    @test plus_eq.head == :(+=)
    @test plus_target isa Expr
    @test plus_target.head == Symbol(".")
    @test plus_target.args[1] == :obj
    @test plus_target.args[2] == QuoteNode(:field)
    @test plus_eq.args[2] == :val

    @test assign isa Expr
    @test assign.head == :(=)
    @test assign_target isa Expr
    @test assign_target.head == Symbol(".")
    @test assign_target.args[1] == :obj
    @test assign_target.args[2] == QuoteNode(:field)
    @test assign.args[2] == :val

    f_string = "field"
    field_access = :($x.$f_string)
    @test field_access.args[2] == QuoteNode("field")
end

true
