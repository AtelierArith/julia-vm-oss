using Test

# Issue #10281 (found while investigating #10133): `Base.float(::Type{T}) where
# {T<:Integer} = T === BigInt ? BigFloat : Float64` returns a RUNTIME
# conditional over the where-bound type parameter, not the parameter itself
# (unlike Issue #9955's `float(::Type{T}) where {T<:AbstractFloat} = T`,
# which directly returns T). Upstream Julia's inference constant-folds the
# `T === BigInt` comparison during a reflection-specialized body-walk (T is
# statically known from the call-site argument type), collapsing the
# ternary to the correct branch and reporting the precise `Type{...}`
# result. sjulia previously widened this to the generic `DataType` because:
#
# 1. `unify_where_params`'s `Type{T}` arm (vm/builtins_reflection/mod.rs)
#    bound T to the *doubly-wrapped* Display string of the call-site type
#    ("Type{Int64}") instead of unwrapping to the inner concrete name
#    ("Int64").
# 2. `try_eval_binary` (compile/const_prop/mod.rs) never folded `===`/`!==`
#    between two `LatticeType::Concrete(ConcreteType::DataType{name})`
#    values (the #9955 static-type-object representation), so the ternary
#    always widened by joining both branches.
#
# Execution was already correct in both engines -- only reflection-time
# inference differed. Issue #10133 extends the same contract through a wrapper
# call: the selected overload's call-independent `MethodSig.return_julia_type`
# snapshot is `Union{DataType,DataType}` and therefore imprecise, so inference
# must analyze that exact method body with the call-site binding for T.

@testset "Issue #10281 infer float(::Type) over a type-level ternary" begin
    @test Base.infer_return_type(float, Tuple{Type{Int64}}) === Type{Float64}
    @test Core.Compiler.return_type(float, Tuple{Type{Int64}}) === Type{Float64}
    @test float(Int64) === Float64

    @test Base.infer_return_type(float, Tuple{Type{BigInt}}) === Type{BigFloat}
    @test Core.Compiler.return_type(float, Tuple{Type{BigInt}}) === Type{BigFloat}
    @test float(BigInt) === BigFloat

    # Regression guard: the AbstractFloat identity branch (direct return,
    # Issue #9955's own case) is unaffected by this ternary-folding fix.
    @test Base.infer_return_type(float, Tuple{Type{Float32}}) === Type{Float32}
    @test float(Float32) === Float32
end

float_int_wrapper_10133() = float(Int64)
float_bigint_wrapper_10133() = float(BigInt)

@testset "Issue #10133 infer wrapper calls through the selected method body" begin
    @test Base.infer_return_type(float_int_wrapper_10133, Tuple{}) === Type{Float64}
    @test Core.Compiler.return_type(float_int_wrapper_10133, Tuple{}) === Type{Float64}
    @test float_int_wrapper_10133() === Float64

    @test Base.infer_return_type(float_bigint_wrapper_10133, Tuple{}) === Type{BigFloat}
    @test Core.Compiler.return_type(float_bigint_wrapper_10133, Tuple{}) === Type{BigFloat}
    @test float_bigint_wrapper_10133() === BigFloat
end

# codex review on PR #10305 found that the `===`-fold must NOT trigger when
# either compared name is ABSTRACT: a where-bound type parameter reflected on
# with an abstract call-site type (`Tuple{Integer}`) binds T to the abstract
# name "Integer", not one specific concrete instantiation -- it denotes a SET
# of possible concrete types, so `T === Int64` is not always false merely
# because the spellings differ (Int64 itself is a member of that set).
# Folding straight to `false` there wrongly pruned the true branch entirely
# instead of reporting the conservative union both branches produce.
g_concrete_10281(x::T) where {T} = T === Int64 ? 1 : "no"

@testset "Issue #10281 review: do not false-fold an abstract type-parameter comparison" begin
    # Abstract call-site type: T could be any Integer subtype, including
    # Int64 itself -- both branches are reachable, so upstream (and this fix)
    # report the conservative union, not just the else branch.
    @test Base.infer_return_type(g_concrete_10281, Tuple{Integer}) == Union{Int64,String}
    # Concrete call-site type: T is statically exactly Int64, so the fold
    # still collapses to the precise true-branch result.
    @test Base.infer_return_type(g_concrete_10281, Tuple{Int64}) === Int64
    @test g_concrete_10281(5) === 1
end

true
