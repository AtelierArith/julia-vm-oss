using Test

# Issue #7733: matrix literals passed as macro arguments must arrive as
# upstream-shaped Expr(:vcat, Expr(:row, ...), ...), not fail quote lowering.

macro matrix_arg_shape7733(ex)
    ok = ex isa Expr &&
         ex.head === :vcat &&
         length(ex.args) == 2 &&
         ex.args[1] isa Expr &&
         ex.args[1].head === :row &&
         ex.args[1].args == [1, 2] &&
         ex.args[2] isa Expr &&
         ex.args[2].head === :row &&
         ex.args[2].args == [3, 4]
    return ok
end

@testset "macro argument matrix literal shape (Issue #7733)" begin
    @test @matrix_arg_shape7733 [1 2; 3 4]
end

@matrix_arg_shape7733 [1 2; 3 4]
