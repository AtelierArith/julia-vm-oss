# Issue #7615: a nested lambda appearing in the RHS (or index) of an
# index-assignment / field-assignment was compiled as a function value but its
# generated function was never registered, because `collect_stmt_functions` had
# no arm for `Stmt::IndexAssign` / `Stmt::FieldAssign`. Execution then failed
# with `Function 'f#__lambda_nested_...' not found`.

using Test

# Lambda in the value of an index assignment (the original report).
function index_assign_value()
    xs = [[1]]
    xs[1] = map(x -> x + 1, xs[1])
    xs[1][1]
end

# Lambda in an index expression of an index assignment.
function index_assign_index()
    xs = [10, 20, 30]
    ys = [1, -2, 3]
    xs[findfirst(x -> x < 0, ys)] = 99
    xs[2]
end

# Lambda in a Dict-style index assignment value (also lowers to IndexAssign).
function dict_index_assign_value()
    d = Dict(:a => [1])
    d[:a] = map(x -> x + 1, d[:a])
    d[:a][1]
end

mutable struct Box7615
    v::Any
end

# Lambda in the value of a field assignment.
function field_assign_value()
    b = Box7615([1])
    b.v = map(x -> x + 1, b.v)
    b.v[1]
end

@testset "nested lambda in index/field assignment (Issue #7615)" begin
    @test index_assign_value() == 2
    @test index_assign_index() == 99
    @test dict_index_assign_value() == 2
    @test field_assign_value() == 2
end

true
