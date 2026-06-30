# Issue #6728: hash is no longer force-intercepted to the Rust BuiltinId; it
# dispatches through normal Julia method dispatch (like isequal/isless), so a
# user-defined 1-arg hash(::T) overload is respected — including when T is used
# as a Dict/Set key (the isequal⇒hash contract). Verified vs julia 1.12.

using Test

struct Pt
    x::Int
    y::Int
end
import Base: hash, isequal, ==
hash(p::Pt) = hash(p.x) ⊻ hash(p.y)
isequal(a::Pt, b::Pt) = isequal(a.x, b.x) && isequal(a.y, b.y)
==(a::Pt, b::Pt) = a.x == b.x && a.y == b.y

@testset "user 1-arg hash/isequal overloads are dispatched (Issue #6728)" begin
    a = Pt(1, 2)
    b = Pt(1, 2)
    c = Pt(3, 4)
    @test isequal(a, b)
    @test !isequal(a, c)
    @test hash(a) == hash(b)        # contract: isequal(a,b) ⇒ hash(a)==hash(b)
    @test hash(a) == (hash(1) ⊻ hash(2))   # user method actually used
end

@testset "user type works as Dict/Set key via custom hash (Issue #6728)" begin
    d = Dict(Pt(1, 2) => "a", Pt(3, 4) => "b")
    @test d[Pt(1, 2)] == "a"        # different instance, same value → found via hash+isequal
    @test d[Pt(3, 4)] == "b"
    @test get(d, Pt(9, 9), "none") == "none"
    s = Set([Pt(1, 2), Pt(3, 4)])
    @test Pt(1, 2) in s
    @test !(Pt(5, 6) in s)
    @test length(Set([Pt(1, 2), Pt(1, 2)])) == 1   # dedup by value
end

@testset "builtin isequal/hash semantics unchanged (Issue #6728)" begin
    @test isequal(NaN, NaN)
    @test isequal(1.0, 1)
    @test hash(1) == hash(1)
    @test hash("abc") == hash("abc")
    @test isequal(missing, missing)
    @test !isequal(missing, 1)
    @test isless(1, 2) && !isless(2, 1)
end

true
