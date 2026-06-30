# Issue #5375: a value-position bounded type-variable method
# `f(x::T) where {T<:Number}` must be ranked MORE specific than an untyped
# fallback `f(x)`, so `f(5)` selects the bounded method regardless of definition
# order.
#
# Root cause: `CoreType::specificity()` scored every type variable as 0 (the
# upper bound was ignored), while an untyped `Any` parameter earned the
# `type_reuse_bonus` in `score_julia_signature_with_binding_count`. The untyped
# fallback therefore out-scored the bounded method (1 vs 0). The fix scores a
# bounded `T<:B` from its bound `B` so it stays as specific as a concrete `B`
# and strictly above `Any`. Reproduces with both the short and long forms (the
# bound is lowered correctly for both), so this is a specificity bug, distinct
# from the long-form bound-drop fixed in #5374.

using Test

# Bounded only (no fallback): already worked, kept as a control.
g(x::T) where {T<:Number} = :g_num

# Bounded defined first, untyped fallback second.
h(x::T) where {T<:Number} = :h_num
h(x) = :h_any

# Untyped fallback first, bounded second (definition order must not matter).
k(x) = :k_any
k(x::T) where {T<:Number} = :k_num

# Long-form spelling of the same competition.
function classify(x::T) where {T<:Number}
    return :num
end
classify(x) = :nonnum

# Short-form spelling.
sclassify(x::T) where {T<:Number} = :num
sclassify(x) = :nonnum

@testset "bounded typevar method outranks untyped fallback (Issue #5375)" begin
    @test g(5) == :g_num
    @test h(5) == :h_num
    @test k(5) == :k_num
    @test classify(5) == :num
    @test sclassify(5) == :num

    # The untyped fallback still wins for non-Number arguments.
    @test h("x") == :h_any
    @test k("x") == :k_any
    @test classify("x") == :nonnum
    @test classify(:sym) == :nonnum
end

# A tighter bound must win over a looser bound for an argument that satisfies
# both: Integer ⊂ Real ⊂ Number.
rank(x::T) where {T<:Number} = :number
rank(x::T) where {T<:Real} = :real
rank(x::T) where {T<:Integer} = :integer

@testset "tighter bound outranks looser bound (Issue #5375)" begin
    @test rank(5) == :integer
    @test rank(2.5) == :real
    @test rank(1 + 2im) == :number
end

# A concrete parameter must still win over a bounded type variable that only
# constrains the argument abstractly: Int64 is strictly more specific than
# `T<:Number` for an Int64 argument.
pick(x::T) where {T<:Number} = :bounded
pick(x::Int64) = :concrete

@testset "concrete parameter still outranks bounded typevar (Issue #5375)" begin
    @test pick(5) == :concrete
    @test pick(2.5) == :bounded
end

# Runtime dispatch: an `Any`-typed element forces method selection at run time
# rather than compile time, exercising the runtime resolver path too.
@testset "bounded typevar wins under runtime dispatch (Issue #5375)" begin
    vals = Any[5, "x", 2.5]
    @test classify(vals[1]) == :num
    @test classify(vals[2]) == :nonnum
    @test classify(vals[3]) == :num
end

true
