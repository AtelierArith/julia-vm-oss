using Test

# Issue #6725 (prevention / consolidation): every native value comparison/hash
# entry point must uniformly resolve heap `Value::StructRef` through the shared
# resolver, so a future op can't silently forget it (the #6685 / #6693 class).
#
# Observable bug before the fix: the native `isequal` builtin did NOT resolve
# heap struct refs, so an immutable struct whose *field* is itself a
# heap-allocated struct (a `Value::StructRef`) compared by `Debug`-string /
# heap-index instead of by value. Two separately-constructed but equal such
# structs got distinct heap indices and `isequal` reported `false`. The `hash`
# / Dict / Set paths already resolved (post-#6693), so the `isequal ⟹ hash`
# contract was broken from the `isequal` side.

struct Inner6725
    v::Int
end
struct Outer6725
    a::Inner6725
    b::Int
end

@testset "isequal resolves nested heap struct refs (Issue #6725)" begin
    # Two separately-constructed equal nested structs (field `a` is a StructRef).
    @test isequal(Outer6725(Inner6725(5), 7), Outer6725(Inner6725(5), 7))
    @test isequal((Outer6725(Inner6725(5), 7),), (Outer6725(Inner6725(5), 7),))
    @test !isequal(Outer6725(Inner6725(5), 7), Outer6725(Inner6725(6), 7))
    @test !isequal(Outer6725(Inner6725(5), 7), Outer6725(Inner6725(5), 8))

    # Single-level immutable structs and tuples of them.
    @test isequal(Inner6725(5), Inner6725(5))
    @test isequal((Inner6725(5), Inner6725(6)), (Inner6725(5), Inner6725(6)))
    @test !isequal((Inner6725(5),), (Inner6725(6),))

    # Base.OneTo is a struct-backed value; isequal over it and tuples of it.
    @test isequal(Base.OneTo(3), Base.OneTo(3))
    @test isequal((Base.OneTo(3), 1), (Base.OneTo(3), 1))
end

@testset "isequal ⟹ hash contract for struct-containing values (Issue #6725)" begin
    # Whenever isequal(x, y) holds, hash(x) == hash(y) must hold.
    x = Outer6725(Inner6725(5), 7)
    y = Outer6725(Inner6725(5), 7)
    @test isequal(x, y)
    @test hash(x) == hash(y)

    tx = (Outer6725(Inner6725(5), 7), Inner6725(9))
    ty = (Outer6725(Inner6725(5), 7), Inner6725(9))
    @test isequal(tx, ty)
    @test hash(tx) == hash(ty)

    # Dict/Set keyed by such composite values rely on this contract.
    d = Dict(tx => 100)
    @test haskey(d, ty)
    @test d[ty] == 100

    s = Set([tx])
    @test ty in s
end

@testset "isless and in over struct-bearing values (Issue #6725)" begin
    # `in` / `∈` over composite struct elements already resolves StructRef
    # (#6691); keep it covered here so the consolidation stays consistent.
    @test (Inner6725(5),) in [(Inner6725(5),), (Inner6725(6),)]
    @test !((Inner6725(9),) in [(Inner6725(5),), (Inner6725(6),)])

    # Scalar `isless` reaches the native `Isless` handler, which now routes its
    # operands through the same shared StructRef resolver as the other native
    # compare/hash entry points (prevention; no observable regression existed
    # for scalars). Behaviour is unchanged for primitives.
    @test isless(1, 2)
    @test !isless(2.0, 1.0)
    @test isless("a", "b")
end

true
