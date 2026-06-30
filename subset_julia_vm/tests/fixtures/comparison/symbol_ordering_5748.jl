using Test

# Issue #5748: Symbols must be orderable (lexicographically by name). Previously
# isless/< /<=/>/cmp/sort/max on Symbols raised "no method matching
# operator(Symbol, Symbol)".

@testset "Symbol ordering (Issue #5748)" begin
    # Core relations
    @test isless(:a, :b)
    @test !isless(:b, :a)
    @test !isless(:a, :a)
    @test :a < :b
    @test :b > :a
    @test :a <= :a
    @test :a >= :a
    @test !(:b < :a)

    # cmp three-way
    @test cmp(:a, :b) == -1
    @test cmp(:b, :a) == 1
    @test cmp(:a, :a) == 0

    # min / max / minmax
    @test min(:b, :a) == :a
    @test max(:a, :b) == :b
    @test minmax(:b, :a) == (:a, :b)

    # sort / sortperm / maximum / minimum
    @test sort([:b, :a, :c]) == [:a, :b, :c]
    @test sort([:foo, :bar, :baz]) == [:bar, :baz, :foo]
    @test sort([:c, :a, :b]; rev=true) == [:c, :b, :a]
    @test sortperm([:c, :a, :b]) == [2, 3, 1]
    @test maximum([:x, :a, :m]) == :x
    @test minimum([:x, :a, :m]) == :a

    # Multi-character names compare by full name
    @test isless(:abc, :abd)
    @test sort([:zz, :z, :a]) == [:a, :z, :zz]

    # A common trigger: sorting Dict symbol keys
    d = Dict(:b => 2, :a => 1, :c => 3)
    @test sort(collect(keys(d))) == [:a, :b, :c]
end

true
