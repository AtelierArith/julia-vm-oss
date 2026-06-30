# Issue #6537: an operator method defined in ASSIGNMENT form with a braced
# `where` clause lost its typevar bound: `*(a::Wrap{T}, b::Wrap{T}) where
# {T<:Real} = ...` lowered as if it were `where {T}`. The function-form
# operator path and the assignment-form non-operator path both kept the bound.
#
# Root cause: `lower_operator_method` (lowering/function/short_form.rs) had a
# hand-rolled where-clause loop that did not recognize the
# BinaryExpression/SubtypeConstraint shapes the pure parser emits for braced
# bounds, so bounded entries were silently dropped from `type_params`. The fix
# routes both the long form and the assignment-form operator path through the
# shared `parse_where_clause_type_params` helper (and converts param
# annotations to TypeVars like the non-operator path does).

using Test

# Separate import lines: `import Base: *, ==, +` fails to parse when a
# comparison operator appears mid-list (Issue #6544).
import Base: *
import Base: ==
import Base: +

struct Wrap6537{T}
    x::T
end

# Assignment-form operator methods (the buggy path).
*(a::Wrap6537{T}, b::Wrap6537{T}) where {T<:Real} = "wrap-real"
*(a::Wrap6537{T}, b::Wrap6537{S}) where {T,S} = "wrap-generic"

# Multi-typevar braces with bounds on both.
+(a::Wrap6537{T}, b::Wrap6537{S}) where {T<:Real,S<:Real} = "plus-real"
+(a::Wrap6537{T}, b::Wrap6537{S}) where {T,S} = "plus-generic"

# `==` spelling.
==(a::Wrap6537{T}, b::Wrap6537{T}) where {T<:Real} = true
==(a::Wrap6537{T}, b::Wrap6537{S}) where {T,S} = false

# Function-form control: same methods, long-form spelling (already worked).
struct WrapCtl6537{T}
    x::T
end
function Base.:*(a::WrapCtl6537{T}, b::WrapCtl6537{T}) where {T<:Real}
    return "ctl-real"
end
function Base.:*(a::WrapCtl6537{T}, b::WrapCtl6537{S}) where {T,S}
    return "ctl-generic"
end

@testset "assignment-form operator keeps braced where bounds (Issue #6537)" begin
    # Runtime dispatch via Any[] so the bound must be enforced at run time.
    wf = Any[Wrap6537("a"), Wrap6537("b")]
    @test wf[1] * wf[2] == "wrap-generic"
    wr = Any[Wrap6537(1), Wrap6537(2)]
    @test wr[1] * wr[2] == "wrap-real"

    # Compile-time (typed) dispatch too.
    @test Wrap6537("a") * Wrap6537("b") == "wrap-generic"
    @test Wrap6537(1) * Wrap6537(2) == "wrap-real"

    # Multi-typevar bounds: both must hold for the bounded method to apply.
    @test Wrap6537(1) + Wrap6537(2.5) == "plus-real"
    @test Wrap6537(1) + Wrap6537("s") == "plus-generic"
    @test Wrap6537("s") + Wrap6537("t") == "plus-generic"

    # `==` spelling.
    @test (Wrap6537(1) == Wrap6537(2)) == true
    @test (Wrap6537("a") == Wrap6537("b")) == false
end

@testset "function-form operator control (Issue #6537)" begin
    cf = Any[WrapCtl6537("a"), WrapCtl6537("b")]
    @test cf[1] * cf[2] == "ctl-generic"
    cr = Any[WrapCtl6537(1), WrapCtl6537(2)]
    @test cr[1] * cr[2] == "ctl-real"
end

# Unbounded braces must stay unbounded (no invented bound).
struct Box6537{T}
    v::T
end
*(a::Box6537{T}, b::Box6537{S}) where {T,S} = "box-any"
@testset "unbounded braced where still matches everything (Issue #6537)" begin
    @test Box6537("x") * Box6537(1) == "box-any"
end

# The UNBRACED form previously failed to parse entirely (`expected Eq`): the
# where-clause constraint was parsed with the general expression parser, which
# swallowed `= body` as an Assignment (Issue #6537).
struct UWrap6537{T}
    x::T
end
*(a::UWrap6537{T}, b::UWrap6537{T}) where T<:Real = "uw-real"
*(a::UWrap6537{T}, b::UWrap6537{S}) where {T,S} = "uw-generic"

@testset "unbraced where on assignment-form operator (Issue #6537)" begin
    @test UWrap6537(1) * UWrap6537(2) == "uw-real"
    @test UWrap6537("a") * UWrap6537("b") == "uw-generic"
    uw = Any[UWrap6537("a"), UWrap6537("b")]
    @test uw[1] * uw[2] == "uw-generic"
end

true
