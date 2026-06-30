# Issue #6738: the reflection predicates isbits / ismutable / hasfield are now
# pure-Julia public wrappers (base/reflection.jl) over the VM-metadata
# primitives isbitstype (type-flag query) / ismutabletype (over _ismutabletype)
# and _fieldnames. Matches upstream julia 1.12 and works as first-class values.
# Migrating ismutable also fixes the prior String divergence (the old Rust
# ismutable returned false for String; upstream and now sjulia return true).

using Test

struct P6738
    x::Int
    y::Float64
end
mutable struct M6738
    a::Int
end

@testset "isbits / isbitstype (Issue #6738)" begin
    @test isbits(5) === true
    @test isbits(2.0) === true
    @test isbits(P6738(1, 2.0)) === true
    @test isbits([1]) === false
    @test isbits("s") === false
    @test isbitstype(Int) === true
    @test isbitstype(P6738) === true
    @test isbitstype(Array) === false
    @test isbitstype(String) === false
end

@testset "ismutable (Issue #6738)" begin
    @test ismutable([1]) === true
    @test ismutable(M6738(1)) === true
    @test ismutable(5) === false
    @test ismutable((1, 2)) === false
    # ismutable(String) is true upstream (was false in the old Rust builtin)
    @test ismutable("s") === true
end

@testset "hasfield (Issue #6738)" begin
    @test hasfield(P6738, :x) === true
    @test hasfield(P6738, :y) === true
    @test hasfield(P6738, :z) === false
    @test hasfield(M6738, :a) === true
    @test hasfield(Int, :x) === false
end

@testset "reflection predicates as first-class values (Issue #6738)" begin
    @test map(isbits, Any[1, [1], 2.0]) == [true, false, true]
    @test map(ismutable, Any[[1], 5]) == [true, false]
    f = isbits
    @test f(5) === true
    g = ismutable
    @test g([1]) === true
end

true
