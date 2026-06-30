# Issue #6731 (slice 2): the mutating Dict ops delete! / get! / empty! / merge!
# route through the pure-Julia Dict{K,V} methods (base/dict.jl) with no
# Value::Dict builtin fallback — their BASE_FUNCTION_ROUTES entries are now
# markers (no BuiltinOp). Verified statically and through a dynamic (Any-typed)
# function barrier, including the #6584 empty!-via-Any trap, and that delete! /
# empty! on a Set still work. Values vs julia 1.12.

using Test

@testset "delete!/get!/empty!/merge! on Dict — pure dispatch (Issue #6731)" begin
    d = Dict(:a => 1, :b => 2)
    delete!(d, :a)
    @test !haskey(d, :a) && haskey(d, :b)
    @test get!(d, :c, 99) == 99 && d[:c] == 99
    @test get!(d, :b, 0) == 2          # existing key: no insert
    merge!(d, Dict(:e => 5))
    @test d[:e] == 5 && length(d) == 3
    empty!(d)
    @test length(d) == 0
end

@testset "mutating ops through a dynamic (Any) barrier — #6584 (Issue #6731)" begin
    function mutate(x)
        x[:new] = 7
        delete!(x, :a)
        get!(x, :z, 3)
        merge!(x, Dict(:m => 1))
        return (haskey(x, :a), x[:new], x[:z], x[:m], length(x))
    end
    @test mutate(Dict(:a => 1, :b => 2)) == (false, 7, 3, 1, 4)
    doempty(x) = (empty!(x); length(x))   # #6584 Any-binding empty! trap
    @test doempty(Dict(:p => 1, :q => 2)) == 0
end

@testset "delete!/empty! on Set still work (Issue #6731)" begin
    s = Set([1, 2, 3])
    delete!(s, 2)
    @test sort(collect(s)) == [1, 3] && length(s) == 2
    empty!(s)
    @test length(s) == 0
end

true
