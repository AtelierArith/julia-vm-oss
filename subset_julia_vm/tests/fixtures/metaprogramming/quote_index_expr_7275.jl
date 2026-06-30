using Test

# Issue #7275: quoting an index expression `:(a[i])` (and the multi-index form
# `:(a[i, j])`) previously failed with "quote for index_expression not yet
# supported". Upstream Julia lowers `a[i]` to `Expr(:ref, a, i...)`, so the quote→
# constructor path must produce the same `:ref` head. This unblocks indexing inside
# macro `quote` bodies (e.g. `Interact.@manipulate`'s and `Plots.@animate`'s
# `esc`-ed loop bodies that index into user data, like `datasets[dataset]`).

# Top-level globals so an eval (Main scope) can resolve the container symbols.
a = [10, 20, 30]
m = [1 2; 3 4]

@testset "quote of index expression has :ref head" begin
    e1 = :(a[2])
    @test e1.head == :ref
    @test e1.args[1] == :a
    @test e1.args[2] == 2

    e2 = :(m[1, 2])
    @test e2.head == :ref
    @test e2.args[1] == :m
    @test e2.args == [:m, 1, 2]
end

@testset "quoted index round-trips through eval" begin
    # The whole point of the fix: a quoted index expr must re-evaluate correctly.
    @test eval(:(a[2])) == 20
    @test eval(:(m[2, 1])) == 3
end

# A nested index `:(d[i][j])` quotes the inner index as the :ref target.
@testset "quote of nested index expression" begin
    nested = :(d[i][j])
    @test nested.head == :ref
    @test nested.args[1] == :(d[i])
    @test nested.args[2] == :j
end

true
