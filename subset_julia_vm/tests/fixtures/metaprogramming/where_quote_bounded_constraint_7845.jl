using Test

# Issue #7845: quoting a value-position `where` type with a *bounded* type
# variable must keep the `<:` / `>:` constraint as a single nested Expr arg
# (`Expr(:<:, :S, :Real)` / `Expr(:comparison, ...)`), not flatten it into
# bare symbol args. The unbounded `where S` form was already correct.
macro double_bounded_tuple_where_7845()
    esc(:(Tuple{T} where Int<:T<:Real))
end

@testset "quote of bounded where keeps <: structure (Issue #7845)" begin
    # Bounded upper-bound constraint: `where S<:Real`.
    e = :(Tuple{T,S} where S<:Real)
    @test e.head === :where
    @test length(e.args) == 2          # body + single constraint, NOT 3
    @test e.args[1] isa Expr           # the `Tuple{T, S}` body
    c = e.args[2]
    @test c isa Expr
    @test c.head === :(<:)
    @test c.args == [:S, :Real]

    # Supertype lower-bound constraint: `where S>:Int`.
    es = :(Tuple{S} where S>:Int)
    @test es.head === :where
    @test length(es.args) == 2
    cs = es.args[2]
    @test cs isa Expr
    @test cs.head === :(>:)
    @test cs.args == [:S, :Int]

    # Double-bounded constraint: `where Int<:T<:Real`.
    db = :(Tuple{T} where Int<:T<:Real)
    @test db.head === :where
    @test length(db.args) == 2
    cdb = db.args[2]
    @test cdb isa Expr
    @test cdb.head === :comparison
    @test cdb.args == [:Int, :(<:), :T, :(<:), :Real]

    # The unbounded `where S` form stays a bare Symbol arg (regression guard).
    u = :(Tuple{T,S} where S)
    @test u.head === :where
    @test length(u.args) == 2
    @test u.args[2] === :S
    @test u.args[2] isa Symbol

    # Braced bounded form `where {S<:Real}` already worked; keep it covered.
    b = :(Tuple{S} where {S<:Real})
    @test b.head === :where
    @test length(b.args) == 2
    cb = b.args[2]
    @test cb isa Expr
    @test cb.head === :(<:)
    @test cb.args == [:S, :Real]

    # A macro returning a bounded `where` preserves the bound structurally.
    me = QuoteNode(:(Tuple{S} where S<:Real))
    mc = eval(me).args[2]
    @test mc isa Expr
    @test mc.head === :(<:)
    @test mc.args == [:S, :Real]

    @test string(@double_bounded_tuple_where_7845()) == "Tuple{T} where Int64<:T<:Real"
end

true
