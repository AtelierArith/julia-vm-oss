# Issue #8121: a parametric struct with an explicit inner constructor gets NO
# synthesized default field constructor in upstream Julia, so a bare `Foo(args)`
# or braces `Foo{T}(args)` call whose arity matches the field count MUST invoke
# the user inner/outer constructor — NOT raw default field construction.
#
# Regression: when the precompiled Base cache was in use, defining a user OUTER
# constructor `Foo(...)` of the same arity made the working method table
# non-empty, which mis-classified the user struct as a cached Base struct and
# SKIPPED registering its inner constructors. Both the bare `Foo(1.0, 2.0)` and
# the braces `Foo{Float64}(1.0, 2.0)` then fell back to default field
# construction (raw store) instead of running the inner ctor body.

# Case 1: inner scales the first field; outer forwards to the braces form.
struct Foo{T}
    a::T
    b::T
    Foo{T}(a, b) where {T} = new{T}(a * 10, b)
end
Foo(a::Number, b::Number) = Foo{Float64}(a, b)

f_bare = Foo(1.0, 2.0)             # outer -> Foo{Float64} -> inner (a*10)
f_braces = Foo{Float64}(3.0, 4.0)  # inner directly
case1 = f_bare.a == 10.0 && f_bare.b == 2.0 && f_braces.a == 30.0 && f_braces.b == 4.0

# Case 2: Rotations-style multi-field normalizing inner constructor.
# AngleAxis{T}(theta, x, y, z) normalizes the (x, y, z) axis to unit length;
# proves the normalizing inner body ran rather than storing the raw axis.
struct AngleAxis{T}
    theta::T
    axis_x::T
    axis_y::T
    axis_z::T
    function AngleAxis{T}(theta, x, y, z) where {T}
        n = sqrt(x * x + y * y + z * z)
        new{T}(theta, x / n, y / n, z / n)
    end
end
AngleAxis(theta::Number, x::Number, y::Number, z::Number) =
    AngleAxis{Float64}(theta, x, y, z)

aa = AngleAxis(0.5, 3.0, 0.0, 4.0)            # axis normalized: (0.6, 0.0, 0.8)
aa2 = AngleAxis{Float64}(1.0, 0.0, 6.0, 8.0)  # braces inner directly: (0.0, 0.6, 0.8)
case2 =
    aa.theta == 0.5 &&
    aa.axis_x == 0.6 && aa.axis_y == 0.0 && aa.axis_z == 0.8 &&
    aa2.axis_x == 0.0 && aa2.axis_y == 0.6 && aa2.axis_z == 0.8

# Case 3: regression guard — Base parametric structs with inner constructors
# (Complex, Rational) must keep working (their cached inner ctors not disturbed).
case3 = Complex{Float64}(2, 3) == 2.0 + 3.0im && (3 // 4) == 3 // 4

# Case 4 (Issue #10959): an ordinary outer constructor may itself carry a
# `where T` binder and recursively call the explicit braces form. The braces
# selector must use constructor identity, not "first same-arity where method",
# or it picks this outer method instead of the normalizing inner constructor.
struct Window10959{T}
    value::T
    offset::Int
    width::Int

    function Window10959{T}(value::T, first::Int, last::Int) where T
        new(value, first - 1, last - first + 1)
    end
end
Window10959(value::T, first::Integer, last::Integer) where T =
    Window10959{T}(value, Int(first) + 10, Int(last) + 10)

window_explicit = Window10959{String}("abc", 2, 3)
window_outer = Window10959("abc", 2, 3)
case4 =
    (window_explicit.offset, window_explicit.width) == (1, 2) &&
    # A bare call must run the outer conversion before the explicit inner call.
    # Selecting the more-specific inner row directly produces (1, 2) here.
    (window_outer.offset, window_outer.width) == (11, 2)

# Case 5: the explicit inner and bare outer can have identical value-parameter
# signatures. Their implicit constructor self types still make them distinct.
struct C10959{T}
    x::T
    C10959{T}(x::T) where T = new(x)
end
C10959(x::T) where T = C10959{T}(x + one(x))
case5 = (C10959{Int}(1).x, C10959(1).x) == (1, 2)

# Case 6: a plain-name inner constructor does not become callable through an
# explicit type application merely because it carries a `where` binder.
struct P10959{T}
    x::T
    P10959(x::T) where T = new{T}(x + one(x))
end
function plain_inner_rejects_explicit_10959()
    try
        P10959{Int}(1)
        return false
    catch err
        return err isa MethodError
    end
end
case6 = plain_inner_rejects_explicit_10959()

# Case 7: overloaded explicit inner constructors dispatch by their value
# argument signatures, not registration order.
struct O10959{T}
    x::T
    O10959{T}(x::Int) where T = new(T(x + 10))
    O10959{T}(x::String) where T = new(T(length(x) + 20))
end
case7 = (O10959{Int}(1).x, O10959{Int}("abc").x) == (11, 23)

# Case 8: a plain inner and a later bare outer have the same constructor self.
# The later outer definition replaces the inner signature, as for any method.
struct PlainRedef10959{T}
    x::T
    PlainRedef10959(x::T) where T = new{T}(x)
end
PlainRedef10959(x::T) where T = x + 100
case8 = PlainRedef10959(1) == 101

# Case 9: braced inner and explicit outer methods form one overload set. The
# concrete Int inner beats the broader Number outer.
struct ExplicitSet10959{T}
    x::T
    ExplicitSet10959{T}(x::Int) where T = new(T(x + 10))
end
ExplicitSet10959{T}(x::Number) where T = ExplicitSet10959{T}(T(x + 100))
case9 = ExplicitSet10959{Int}(1).x == 11

# Case 10: method-local binders do not consume explicit self type arguments.
# S is inferred from x; only T is bound from `{Float64}`.
struct LocalBinder10959{T}
    x::T
    LocalBinder10959{T}(x::S) where {S,T} = new(T(x))
end
local_binder = LocalBinder10959{Float64}(1)
case10 = local_binder.x == 1.0 && typeof(local_binder) == LocalBinder10959{Float64}

# Case 11: imprecise value-argument inference must not reject the only eligible
# explicit inner constructor. Runtime overload selection is only required when
# more than one candidate remains.
struct SingleCandidate10959{T}
    x::T
    SingleCandidate10959{T}(x::Number) where T = new(T(x + 1))
end
single_candidate_10959(x) = SingleCandidate10959{Int}(x)
case11 = single_candidate_10959(1).x == 2

# Case 12: a general runtime self expression uses ApplyTypeDynamic. Preserve the
# braced constructor self in FunctionInfo so the resulting DataType call can
# discover and bind the inner constructor.
struct DynamicSelf10959{T}
    x::T
    marker::Int
    DynamicSelf10959{T}(x) where T = new(T(x), 7)
end
dynamic_self_10959(x) = DynamicSelf10959{typeof(x)}(x)
dynamic_self_value = dynamic_self_10959(2)
case12 =
    dynamic_self_value.x == 2 &&
    dynamic_self_value.marker == 7 &&
    typeof(dynamic_self_value) == DynamicSelf10959{Int64}

# Case 13: a concrete constructor self is a real constraint, not an empty
# binder list. `ConcreteSelf10959{String}` must not reach the `{Int}` inner.
struct ConcreteSelf10959{T}
    x::T
    ConcreteSelf10959{Int}(x) = new{Int}(x)
end
function concrete_self_rejects_other_10959()
    try
        ConcreteSelf10959{String}(1)
        return false
    catch err
        return err isa MethodError
    end
end
case13 =
    ConcreteSelf10959{Int}(1).x == 1 &&
    concrete_self_rejects_other_10959()

# Case 14: repeated self binders are diagonal constraints. Enforce them in
# both static and ApplyTypeDynamic candidate selection.
struct DiagonalSelf10959{A,B}
    x::A
    marker::Int
    DiagonalSelf10959{T,T}(x) where T = new{T,T}(x, 7)
end
function diagonal_self_rejects_mixed_10959()
    try
        DiagonalSelf10959{Int,String}(1)
        return false
    catch err
        return err isa MethodError
    end
end
dynamic_diagonal_self_10959(x, y) = DiagonalSelf10959{typeof(x),typeof(y)}(x)
function dynamic_diagonal_self_rejects_mixed_10959()
    try
        dynamic_diagonal_self_10959(1, "x")
        return false
    catch err
        return err isa MethodError
    end
end
case14 =
    typeof(DiagonalSelf10959{Int,Int}(1)) == DiagonalSelf10959{Int,Int} &&
    diagonal_self_rejects_mixed_10959() &&
    typeof(dynamic_diagonal_self_10959(1, 2)) == DiagonalSelf10959{Int64,Int64} &&
    dynamic_diagonal_self_rejects_mixed_10959()

# Case 15: runtime constructor-self names stay module-qualified. Same-named
# parametric structs in different modules must not share ApplyType candidates.
module ConstructorModuleA10959
struct Box10959{T}
    x::T
    marker::Int
    Box10959{T}(x) where T = new{T}(x, 1)
end
make10959(x) = Box10959{typeof(x)}(x)
end
module ConstructorModuleB10959
struct Box10959{T}
    x::T
    marker::Int
    Box10959{T}(x) where T = new{T}(x, 2)
end
make10959(x) = Box10959{typeof(x)}(x)
end
module_box_a = ConstructorModuleA10959.make10959(1)
module_box_b = ConstructorModuleB10959.make10959(1)
case15 = module_box_a.marker == 1 && module_box_b.marker == 2

# Case 16: the explicit self binding participates in value-signature dispatch.
# `{String}` cannot independently re-infer the same T as Int from x.
struct CoupledSelf10959{T}
    x::T
    CoupledSelf10959{T}(x::T) where T = new(x)
end
function coupled_self_rejects_mismatch_10959()
    try
        CoupledSelf10959{String}(1)
        return false
    catch err
        return err isa MethodError
    end
end
case16 = coupled_self_rejects_mismatch_10959()

# Case 17: the single-candidate escape is only for imprecise inference. A
# concrete Int argument must not bypass a declared String parameter.
struct DefiniteMismatch10959{T}
    x::T
    DefiniteMismatch10959{T}(x::String) where T = new(T(42))
end
function definite_mismatch_rejected_10959()
    try
        DefiniteMismatch10959{Int}(1)
        return false
    catch err
        return err isa MethodError
    end
end
case17 = definite_mismatch_rejected_10959()

# Case 18: self binders keep their upper bounds on forwarded static parameters
# and ApplyTypeDynamic runtime expressions.
struct BoundedSelf10959{T}
    x::T
    BoundedSelf10959{T}(x) where {T<:Number} = new{T}(x)
end
bounded_forward_10959(x::T) where T = BoundedSelf10959{T}(x)
bounded_runtime_10959(x) = BoundedSelf10959{typeof(x)}(x)
function bounded_forward_rejects_string_10959()
    try
        bounded_forward_10959("oops")
        return false
    catch err
        return err isa MethodError
    end
end
function bounded_runtime_rejects_string_10959()
    try
        bounded_runtime_10959("oops")
        return false
    catch err
        return err isa MethodError
    end
end
case18 =
    bounded_forward_10959(1).x == 1 &&
    bounded_runtime_10959(2).x == 2 &&
    bounded_forward_rejects_string_10959() &&
    bounded_runtime_rejects_string_10959()

# Case 19: a qualified module constructor must never pool with an unqualified
# top-level constructor of the same short name.
struct MixedOwnerBox10959{T}
    x::T
    marker::Int
    MixedOwnerBox10959{T}(x::Int) where T = new{T}(x, 0)
end
module MixedOwnerModule10959
struct MixedOwnerBox10959{T}
    x::T
    marker::Int
    MixedOwnerBox10959{T}(x) where T = new{T}(x, 1)
end
make_dynamic_10959(x) = MixedOwnerBox10959{typeof(x)}(x)
make_static_10959() = MixedOwnerBox10959{Int}(1)
end
mixed_owner_dynamic = MixedOwnerModule10959.make_dynamic_10959(1)
mixed_owner_static = MixedOwnerModule10959.make_static_10959()
case19 =
    mixed_owner_dynamic.marker == 1 &&
    mixed_owner_static.marker == 1 &&
    typeof(mixed_owner_dynamic) == MixedOwnerModule10959.MixedOwnerBox10959{Int64}

# Case 20: binder spelling is alpha-equivalent constructor identity, so the
# second definition replaces the first.
struct AlphaSelf10959{A}
    x::A
    AlphaSelf10959{T}(x::T) where T = new{T}(x + 1)
    AlphaSelf10959{S}(x::S) where S = new{S}(x + 2)
end
case20 = AlphaSelf10959{Int}(1).x == 3

# Case 21: constructor self patterns compare type identity, not alias spelling.
struct AliasSelf10959{T}
    x::Int
    AliasSelf10959{Vector{S}}(x::Int) where S = new{Vector{S}}(x)
end
alias_self_value = AliasSelf10959{Array{Int,1}}(1)
case21 = typeof(alias_self_value) == AliasSelf10959{Vector{Int64}}

# Case 22: runtime apply_type expands const Union aliases in declared bounds,
# and reduced-arity stubs for positional defaults forward the runtime self
# binding into the full explicit inner constructor (Issue #11003).
const AliasElem11003 = Union{Integer,String}
struct AliasDefault11003{T<:AliasElem11003}
    value::T
    marker::Bool
    function AliasDefault11003{T}(value::T, marker::Bool=true) where T<:AliasElem11003
        new{T}(value, marker)
    end
end
alias_default_11003(x) = Core.apply_type(AliasDefault11003, typeof(x))(x)
alias_default_int = alias_default_11003(7)
alias_default_string = alias_default_11003("ok")
case22 =
    typeof(alias_default_int) == AliasDefault11003{Int64} &&
    alias_default_int.marker &&
    typeof(alias_default_string) == AliasDefault11003{String} &&
    alias_default_string.marker

# Case 23: self alpha-normalization must preserve how self slots correlate with
# value-parameter binders. These methods are distinct, not redefinitions.
struct CrossBind10959{A,B}
    CrossBind10959{T,S}(x::T) where {T,S} = :first
    CrossBind10959{S,T}(x::T) where {T,S} = :second
end
case23 =
    CrossBind10959{Int,String}(1) == :first &&
    CrossBind10959{Int,String}("x") == :second

# Case 24: alias-equivalent self signatures are the same method identity, so
# the later Array spelling replaces the earlier Vector spelling.
struct AliasRedef10959{T}
    AliasRedef10959{Vector{S}}() where S = :first
    AliasRedef10959{Array{S,1}}() where S = :second
end
case24 = AliasRedef10959{Vector{Int}}() == :second

# Case 25: an unqualified top-level explicit outer must not enter a qualified
# module type's overload set merely because their short type names agree.
struct OwnerLeak10959{T}
    OwnerLeak10959{T}(x::String) where T = new{T}()
end
OwnerLeak10959{T}(x::Int) where T = :top
module OwnerLeakModule10959
struct OwnerLeak10959{T}
    marker::Int
    OwnerLeak10959{T}(x::Number) where T = new{T}(1)
end
make_owner_10959() = OwnerLeak10959{Int}(1)
end
case25 = OwnerLeakModule10959.make_owner_10959() isa OwnerLeakModule10959.OwnerLeak10959{Int}

# Case 26: a structured self type must still satisfy its declared upper bound.
struct ParamBound10959{T}
    ParamBound10959{T}() where {T<:Number} = :hit
end
function parameterized_self_bound_rejects_10959()
    try
        ParamBound10959{Vector{Int}}()
        return false
    catch err
        return err isa MethodError
    end
end
case26 = parameterized_self_bound_rejects_10959()

# Case 27: instantiating the self binder A must also substitute A in the bound
# of the surviving value-parameter binder B.
struct DependentCtorBound10959{A}
    DependentCtorBound10959{A}(x::B) where {A,B<:A} = :hit
end
case27 = DependentCtorBound10959{Number}(1) == :hit

# Case 28: materializing a concrete instantiation does not resurrect the
# suppressed raw field constructor after runtime DataType dispatch misses.
struct RuntimeMiss10959{T}
    x::T
    RuntimeMiss10959{T}(x::String) where T = new{T}(T(length(x)))
end
RuntimeMiss10959{Int}("a")
function runtime_miss_rejects_field_constructor_10959()
    ctor = Any[RuntimeMiss10959{Int}][1]
    try
        ctor(1)
        return false
    catch err
        return err isa MethodError
    end
end
case28 = runtime_miss_rejects_field_constructor_10959()

# Case 29: short-form positional defaults contribute a full-arity method and a
# reduced-arity forwarding inner-constructor stub.
struct ShortDefault10959{T}
    x::Bool
    ShortDefault10959{T}(x=true) where T = new{T}(x)
end
case29 = ShortDefault10959{Int}().x

# Case 30: Base-qualified built-in aliases canonicalize with their bare forms.
struct QualifiedAlias10959{T}
    QualifiedAlias10959{Base.Vector{S}}() where S = :hit
end
case30 = QualifiedAlias10959{Array{Int,1}}() == :hit

# Case 31: constructor-self validation enforces lower as well as upper bounds.
struct LowerBoundSelf10959{T}
    LowerBoundSelf10959{T}() where {T>:Integer} = :hit
end
function lower_bound_rejects_narrow_self_10959()
    try
        LowerBoundSelf10959{Int8}()
        return false
    catch err
        return err isa MethodError
    end
end
case31 =
    LowerBoundSelf10959{Number}() == :hit &&
    lower_bound_rejects_narrow_self_10959()

# Case 32: owner isolation is independent of definition order. Registering a
# module type first must not make a later top-level type's bare name resolve to
# the module owner.
module EarlyOwnerModule10959
struct ReverseOwner10959{T}
    ReverseOwner10959{T}(x::Number) where T = :module_inner
end
make_reverse_owner_10959() = ReverseOwner10959{Int}(1)
end
struct ReverseOwner10959{T}
    ReverseOwner10959{T}(x::String) where T = :top_inner
end
ReverseOwner10959{T}(x::Int) where T = :top_outer
case32 =
    ReverseOwner10959{Int}(1) == :top_outer &&
    EarlyOwnerModule10959.make_reverse_owner_10959() == :module_inner

# Case 33: user aliases participate in constructor-self method identity. The
# later alias spelling replaces the earlier direct spelling of the same self.
const UserVectorAlias11019 = Vector
struct UserAliasSelf11019{T}
    UserAliasSelf11019{Vector{S}}() where S = :first
    UserAliasSelf11019{UserVectorAlias11019{S}}() where S = :second
end
case33 = UserAliasSelf11019{Vector{Int}}() == :second

# Case 34: bounds on binders used only by the implicit constructor self remain
# part of method identity; neither bounded family redefines the other.
struct BoundIdentity11019{T}
    BoundIdentity11019{T}() where {T<:Number} = :num
    BoundIdentity11019{T}() where {T<:AbstractString} = :str
end
case34 =
    BoundIdentity11019{Int}() == :num &&
    BoundIdentity11019{String}() == :str

# Case 35: a bare module-local type in a constructor self pattern resolves to
# the same qualified identity carried by the runtime DataType call.
module OwnerArgModule11019
struct Tag end
struct Box{T}
    Box{Tag}() = :ok
end
end
case35 = OwnerArgModule11019.Box{OwnerArgModule11019.Tag}() == :ok

# Case 36: a same-named alias in another module must not make the constructor's
# lexical alias ambiguous or redirect its self identity.
module LexicalAliasModule11019
const SharedAlias11019 = Vector
struct Box{T}
    Box{SharedAlias11019{S}}() where S = :ok
end
end
module OtherAliasModule11019
const SharedAlias11019 = Matrix
end
case36 = LexicalAliasModule11019.Box{Vector{Int}}() == :ok

# Case 37: same-named bounds owned by different modules remain distinct parts
# of constructor method identity.
module BoundOwnerA11019
abstract type Bound end
struct Value <: Bound end
end
module BoundOwnerB11019
abstract type Bound end
struct Value <: Bound end
end
struct QualifiedBoundSelf11019{T}
    QualifiedBoundSelf11019{T}() where {T<:BoundOwnerA11019.Bound} = :a
    QualifiedBoundSelf11019{T}() where {T<:BoundOwnerB11019.Bound} = :b
end
case37 =
    QualifiedBoundSelf11019{BoundOwnerA11019.Value}() == :a &&
    QualifiedBoundSelf11019{BoundOwnerB11019.Value}() == :b

# Case 38: bare and qualified spellings of one lexical module bound identify
# the same method, so the later qualified spelling replaces the first.
module LexicalBoundSpelling11019
abstract type Bound end
struct C{T}
    C{T}() where {T<:Bound} = :first
    C{T}() where {T<:LexicalBoundSpelling11019.Bound} = :second
end
struct Value <: Bound end
end
# Bare type-argument scope inside a module body is tracked separately by #11034.
case38 =
    LexicalBoundSpelling11019.C{LexicalBoundSpelling11019.Value}() == :second

# Case 39: dependent structured bounds substitute constructor-self binders at
# every nesting depth, on both static and runtime DataType routes.
struct NestedDependentBound11019{A,B}
    NestedDependentBound11019{A,B}() where {A,B<:Vector{A}} = :hit
end
nested_dependent_runtime_11019(x) =
    NestedDependentBound11019{typeof(x),Vector{typeof(x)}}()
case39 =
    NestedDependentBound11019{Int,Vector{Int}}() == :hit &&
    nested_dependent_runtime_11019(1) == :hit

# Case 40: selecting a sole constructor candidate because the compile-time
# argument is Any still validates its declared value signature at runtime.
struct StaticAnyValidation11019{T}
    StaticAnyValidation11019{T}(x::String) where T = :string
end
static_any_validation_11019(x) = StaticAnyValidation11019{Int}(x)
function static_any_rejects_int_11019()
    try
        static_any_validation_11019(1)
        return false
    catch err
        return err isa MethodError
    end
end
case40 =
    static_any_validation_11019("ok") == :string &&
    static_any_rejects_int_11019()

# Case 41: independent where-binder declaration order is not method identity;
# the later alpha-equivalent constructor replaces the earlier one.
struct BinderOrder11019{A,B}
    BinderOrder11019{T,S}() where {T,S} = :first
    BinderOrder11019{T,S}() where {S,T} = :second
end
case41 = BinderOrder11019{Int,String}() == :second

ok =
    case1 && case2 && case3 && case4 && case5 &&
    case6 && case7 && case8 && case9 && case10 &&
    case11 && case12 && case13 && case14 && case15 &&
    case16 && case17 && case18 && case19 && case20 && case21 && case22 &&
    case23 && case24 && case25 && case26 && case27 && case28 && case29 &&
    case30 && case31 && case32 && case33 && case34 && case35 && case36 &&
    case37 && case38 && case39 && case40 && case41
println(ok)
ok
