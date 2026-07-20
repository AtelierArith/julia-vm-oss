# Issue #5174: Symbol/string interning.
#
# `SymbolValue` is now backed by an interned `Rc<str>` (a thread-local
# intern table). Construction reuses a shared allocation per distinct name
# and equality takes a pointer-equality fast path. This must not change any
# observable Julia-level Symbol semantics: identity (`===`), equality,
# hashing (Dict / Set keys), conversion to/from strings, and `Val{:sym}`
# dispatch all stay byte-identical to upstream Julia.

using Test

@testset "Symbol identity and equality (Issue #5174)" begin
    a = :alpha
    b = Symbol("alpha")
    c = :beta
    # Interned equal names compare equal and are `===`.
    @test a == b
    @test a === b
    @test a != c
    @test !(a === c)
end

@testset "Symbol <-> String round trip (Issue #5174)" begin
    s = :round_trip_name
    @test string(s) == "round_trip_name"
    @test Symbol(string(s)) === s
    @test string(:xyz) == "xyz"
end

@testset "Symbols as Dict keys hash consistently (Issue #5174)" begin
    d = Dict(:one => 1, :two => 2)
    d[Symbol("three")] = 3
    @test d[:one] == 1
    @test d[:two] == 2
    @test d[Symbol("three")] == 3
    @test haskey(d, :three)
    @test !haskey(d, :four)
    @test length(d) == 3
end

@testset "Symbols in a Set deduplicate by value (Issue #5174)" begin
    s = Set([:x, :y, Symbol("x"), :z, Symbol("y")])
    @test length(s) == 3
    @test :x in s
    @test Symbol("z") in s
    @test !(:w in s)
end

@testset "Symbol type parameters compare equal (Issue #5174)" begin
    # Symbols carried inside a `Val` type parameter rely on Symbol equality;
    # interning must keep `Val(:tag)` types equal regardless of how the
    # symbol was built.
    @test Val(:tag) == Val(:tag)
    @test Val(:tag) == Val(Symbol("tag"))
    @test typeof(Val(:tag)) == typeof(Val(Symbol("tag")))
    @test Val(:tag) != Val(:other)
end

@testset "Many programmatically built symbols stay distinct (Issue #5174)" begin
    syms = [Symbol("col_", i) for i in 1:5]
    @test syms[1] === :col_1
    @test syms[5] === :col_5
    @test length(Set(syms)) == 5
    # Rebuilding the same name yields an equal (interned) symbol.
    @test Symbol("col_", 3) === syms[3]
end

true
