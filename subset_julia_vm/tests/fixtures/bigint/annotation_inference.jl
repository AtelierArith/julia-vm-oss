# Test that ::BigInt / ::BigFloat / ::Symbol / ::Nothing / ::Missing
# annotations preserve their declared type during VM type inference,
# instead of degrading to Top/Any (Issue #3531).

using Test

function id_bigint(x::BigInt)
    return x
end

function id_bigfloat(x::BigFloat)
    return x
end

function id_symbol(x::Symbol)
    return x
end

function id_nothing(x::Nothing)
    return x
end

function id_missing(x::Missing)
    return x
end

@testset "annotation preservation (Issue #3531)" begin
    @test id_bigint(big"42") == big"42"
    @test id_bigfloat(big"1.25") == big"1.25"
    @test id_symbol(:foo) === :foo
    @test id_nothing(nothing) === nothing
    @test ismissing(id_missing(missing))
end

true
