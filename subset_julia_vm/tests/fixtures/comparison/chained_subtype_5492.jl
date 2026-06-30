# Test chained subtype comparisons (Issue #5492)
# Julia expands `A <: B <: C` to `(A <: B) && (B <: C)`, evaluating the
# middle operand once. The same chaining applies to the `>:` supertype
# operator. Previously sjulia parsed `A <: B <: C` as `(A <: B) <: C`,
# i.e. `Bool <: C`, returning the wrong answer.

using Test

@testset "Chained subtype: A <: B <: C expanded to (A <: B) && (B <: C) (Issue #5492)" begin

    # `<:` chains (true / false)
    r1 = Int <: Real <: Number              # true: (Int <: Real) && (Real <: Number)
    r2 = Int <: String <: Any               # false: (Int <: String) == false short-circuits
    r3 = Float64 <: Real <: Number <: Any   # true: longer chain, all links hold

    # `>:` (supertype) chains
    r4 = Any >: Number >: Int               # true: (Any >: Number) && (Number >: Int)
    r5 = Number >: Int >: String            # false: (Int >: String) == false

    # Mixed `<:` / `>:` chain
    r6 = Int <: Integer >: Bool             # true: (Int <: Integer) && (Integer >: Bool)

    # Single-op forms must still work unchanged
    r7 = Int <: Real                        # true
    r8 = Any >: Int                          # true

    @test r1
    @test !r2
    @test r3
    @test r4
    @test !r5
    @test r6
    @test r7
    @test r8
end

true  # Test passed
