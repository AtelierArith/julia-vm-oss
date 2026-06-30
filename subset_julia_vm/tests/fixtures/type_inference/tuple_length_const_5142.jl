using Test

# Issue #5142: the length of a fixed-arity tuple is a statically known value.
# Inference now propagates it as Const(N) (mirroring upstream `nfields_tfunc`),
# which enables constant folding and branch elimination while keeping the
# observable runtime result identical to upstream Julia.

# Length of a known-arity tuple.
tuple_len_3() = length((1, 2.0, "three"))

# Constant folding on top of a known tuple length: the whole expression is a
# compile-time constant, but must still evaluate to the same runtime value.
tuple_len_plus_one() = length((10, 20)) + 1

# Branch selected by a known tuple length. The condition is statically known,
# so dead branches can be eliminated; the chosen branch must still run.
function classify_pair(t)
    if length(t) == 2
        return :pair
    else
        return :other
    end
end

# Empty tuple has length 0.
empty_tuple_len() = length(())

@testset "tuple length constant propagation (Issue #5142)" begin
    @test tuple_len_3() == 3
    @test tuple_len_plus_one() == 3
    @test classify_pair((1, 2)) == :pair
    @test classify_pair((1, 2, 3)) == :other
    @test empty_tuple_len() == 0

    # length of a literal tuple used directly in an expression.
    @test length((1, 2, 3, 4)) == 4
    @test ntuple(identity, 3) == (1, 2, 3)
    @test length(ntuple(identity, 3)) == 3
end

true
