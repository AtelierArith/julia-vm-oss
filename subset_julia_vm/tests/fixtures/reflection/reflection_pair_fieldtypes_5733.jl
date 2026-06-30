using Test

# Issue #5733: fieldtypes/fieldtype on a Pair{A,B} type returned (Any, Any) — sjulia
# represents Pair as a non-parametric struct, so its declared field types are
# untyped. They are now resolved from the type arguments: first::A, second::B.

@testset "fieldtypes/fieldtype on Pair{A,B} (Issue #5733)" begin
    @test fieldtypes(Pair{Int,String}) == (Int64, String)
    @test fieldtype(Pair{Int,String}, 1) == Int64
    @test fieldtype(Pair{Int,String}, 2) == String
    @test fieldtype(Pair{Int,String}, :first) == Int64
    @test fieldtype(Pair{Int,String}, :second) == String
    @test fieldtypes(Pair{Float64,Int}) == (Float64, Int64)
    @test fieldtypes(Pair{String,Vector{Int}}) == (String, Vector{Int64})
    @test fieldtypes(Pair{Symbol,Int}) == (Symbol, Int64)

    # Bare Pair (no parameters) is unchanged.
    @test fieldtypes(Pair) == (Any, Any)

    # User parametric structs and Complex are unaffected (regression).
    @test fieldtypes(Complex{Float64}) == (Float64, Float64)

    # fieldnames still consistent.
    @test fieldnames(Pair{Int,String}) == (:first, :second)
end

true
