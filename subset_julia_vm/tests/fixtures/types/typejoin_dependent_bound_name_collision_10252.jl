# Dependent bounds must reference the same owner-scoped TypeVar object as the
# binder they name, including after another struct reuses the same parameter
# spellings (Issues #10252/#10261). The regression checks both reflection
# identity and exact typejoin precision; the older soundness-only fallback is
# no longer sufficient.

using Test

struct Dep2_10252{A, B<:A} end
struct Dep3_10252{A, B<:A, C<:B} end

# Hoisted to the top level rather than nested inside the `@testset` below:
# a `struct` definition inside `@testset` fails to lower in sjulia (Issue
# #10194, same known gap noted in typejoin_partial_param_widen_10091.jl).
struct DepIso10252{P, Q<:P, R<:Q} end
struct DepComposite10261{A, B<:Vector{A}} end
struct DepLower10261{A, B>:A} end
struct DepBoundedOuter10261{A<:Number, B<:A} end

@testset "dependent-bound reflection identity (Issue #10261)" begin
    w2 = Dep2_10252
    a2 = w2.var
    b2 = w2.body.var
    @test b2.ub === a2

    w3 = Dep3_10252
    a3 = w3.var
    b3 = w3.body.var
    c3 = w3.body.body.var
    @test b3.ub === a3
    @test c3.ub === b3

    # Observe the suffix first. A bounded outer binder must still be projected
    # before its body is exposed, so B.ub reuses that later-reflected A object.
    bounded_b = DepBoundedOuter10261.body.var
    bounded_a = DepBoundedOuter10261.var
    @test bounded_b.ub === bounded_a
end

@testset "apply_type defers bounds containing a free TypeVar (Issue #10261)" begin
    a = TypeVar(:A)
    @test Core.apply_type(DepComposite10261, a, Vector{Int64}) === DepComposite10261{a,Vector{Int64}}
    @test Core.apply_type(DepComposite10261, a, Int64) === DepComposite10261{a,Int64}
    f = TypeVar(:F, Union{}, Real)
    @test Core.apply_type(Dep2_10252, f, String) === Dep2_10252{f,String}
    @test Core.apply_type(DepLower10261, f, String) === DepLower10261{f,String}

    # `within_typevar` checks the argument before substituting an earlier
    # binder. Both invariant upper and lower bounds therefore accept a value
    # that still contains the free `a`, even after the first slot consumes Int.
    upper_b = TypeVar(:B, Union{}, Vector{String})
    upper_w = UnionAll(a, UnionAll(upper_b, Tuple{a,upper_b}))
    upper_result = Core.apply_type(upper_w, Int, Vector{a})
    # Whole-Tuple type identity for this runtime construction path is tracked
    # separately by Issue #10861; pin the #10261 contract at each parameter.
    @test upper_result.parameters[1] === Int
    @test upper_result.parameters[2] === Vector{a}
    @test upper_result.parameters[2].parameters[1] === a

    lower_b = TypeVar(:B, Vector{String}, Any)
    lower_w = UnionAll(a, UnionAll(lower_b, Tuple{a,lower_b}))
    lower_result = Core.apply_type(lower_w, Int, Vector{a})
    @test lower_result.parameters[1] === Int
    @test lower_result.parameters[2] === Vector{a}
    @test lower_result.parameters[2].parameters[1] === a

    # Upstream skips the complete endpoint check when either endpoint still
    # contains a free TypeVar, even if the opposite concrete endpoint alone
    # would reject this argument.
    endpoint_f = TypeVar(:EndpointF, Union{}, Real)
    endpoint_b = TypeVar(:EndpointB, endpoint_f, Real)
    endpoint_w = UnionAll(endpoint_b, Tuple{endpoint_b})
    @test Core.apply_type(endpoint_w, String).parameters[1] === String

    rank_t = TypeVar(:RankT)
    rank3 = Array{rank_t,3}
    @test rank3.parameters[1] === rank_t
    @test rank3.parameters[2] === 3
end

@testset "typejoin - dependent bound stays sound across name-colliding structs (Issue #10252)" begin
    # Step 1 (mirrors the issue's own MWE): exercise Dep2_10252's "both
    # differ" widening path once. Both type parameters differ between the
    # two instantiations, so there is nothing concrete left to substitute
    # into a dependent bound -- upstream AND sjulia both collapse this one to
    # the bare wrapper.
    r2 = typejoin(Dep2_10252{Int64,Int64}, Dep2_10252{Float64,Float64})
    @test r2 === Dep2_10252
    @test Dep2_10252{Int64,Int64} <: r2
    @test Dep2_10252{Float64,Float64} <: r2

    # Step 2: Dep3_10252 is a DIFFERENT, independently-declared struct that
    # reuses the SAME parameter names (A, B) as Dep2_10252 above, plus its
    # own dependent C<:B. Running Dep2_10252's typejoin first (step 1) is
    # what triggers the name-keyed cache collision described above.
    #
    # Regardless of the collision, the result MUST remain a sound upper
    # bound of both operands -- this is the property the #10225 safety net
    # (the `<:`-verified fallback in `_typejoin_subst_dependent_bound`'s
    # caller) exists to guarantee.
    r4 = typejoin(Dep3_10252{Number,Int64,Int64}, Dep3_10252{Number,Float64,Float64})
    @test Dep3_10252{Number,Int64,Int64} <: r4
    @test Dep3_10252{Number,Float64,Float64} <: r4
    # r4 is also bounded by the wrapper itself (never widens past what the
    # struct declaration allows).
    @test r4 <: Dep3_10252
    @test r4 === (Dep3_10252{Number,B,C} where {B<:Number, C<:B})
end

@testset "typejoin - dependent bound in isolation still matches upstream exactly (Issue #10252 baseline)" begin
    # Sanity check / regression guard: WITHOUT a prior name-colliding call,
    # Dep3_10252's own dependent-bound widening is exact (matches the
    # already-covered DepTriple10091 case in
    # typejoin_partial_param_widen_10091.jl), confirming the collision above
    # is specifically about cross-struct name reuse, not a general defect in
    # the dependent-bound substitution itself.
    r = typejoin(DepIso10252{Number,Int64,Int64}, DepIso10252{Number,Float64,Float64})
    @test r === (DepIso10252{Number,Q,R} where {Q<:Number, R<:Q})
    @test DepIso10252{Number,Int64,Int64} <: r
    @test DepIso10252{Number,Float64,Float64} <: r
end

true
