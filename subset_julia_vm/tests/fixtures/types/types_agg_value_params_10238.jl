# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: types/nested_ntuple_value_param_binding_4842.jl =====
module Agg_nested_ntuple_value_param_binding_4842
using Test

# Nested NTuple value-parameter binding (Issue #4842).
# In `NTuple{N, NTuple{M, T}}` the inner length value parameter `M` and the
# element type parameter `T` must be bound in the method frame, not just the
# outer length `N`.
hn_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = (N, M, T)
hn_inner_len_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = M
hn_inner_elem_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = T
hn_outer_len_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = N
hn_sum_first_4842(xs::NTuple{N,NTuple{M,T}}) where {N,M,T} = sum(map(t -> t[1], xs))

# Triple nesting: every length parameter must bind.
deep_4842(xs::NTuple{A,NTuple{B,NTuple{C,T}}}) where {A,B,C,T} = (A, B, C, T)

@testset "Nested NTuple value-parameter binding (Issue #4842)" begin
    @test hn_4842(((1, 2, 3), (4, 5, 6))) == (2, 3, Int64)
    @test hn_outer_len_4842(((1, 2, 3), (4, 5, 6))) == 2
    @test hn_inner_len_4842(((1, 2, 3), (4, 5, 6))) == 3
    @test hn_inner_elem_4842(((1, 2, 3), (4, 5, 6))) == Int64
    @test hn_4842(((1.0, 2.0), (3.0, 4.0), (5.0, 6.0))) == (3, 2, Float64)
    @test hn_sum_first_4842(((10, 1), (20, 2), (30, 3))) == 60
    @test deep_4842((((1, 2), (3, 4)),)) == (1, 2, 2, Int64)
end
end # module Agg_nested_ntuple_value_param_binding_4842

# ===== source: types/nested_param_constructor_4851.jl =====
module Agg_nested_param_constructor_4851
using Test

# Issue #4851: Parametric default constructor inference must bind type
# parameters embedded inside nested field type expressions like Tuple{T,T}
# or Vector{T}, not just bare `field::T` fields.

struct NestedParamProbe4851{T}
    a::Tuple{T,T}
    b::String
end

struct VecParamProbe4851{T}
    a::Vector{T}
end

struct PairParamProbe4851{S,T}
    a::Tuple{S,T}
end

mk_nested_4851(flag) = NestedParamProbe4851((flag ? 1 : 2, 3), "y")
getb_nested_4851(flag) = getfield(mk_nested_4851(flag), :b)

@testset "Nested parametric constructor inference (Issue #4851)" begin
    v = mk_nested_4851(true)
    @test v isa NestedParamProbe4851
    @test v isa NestedParamProbe4851{Int64}
    @test typeof(v) == NestedParamProbe4851{Int64}
    @test v.a == (1, 3)
    @test typeof(v.a) == Tuple{Int64, Int64}
    @test v.b == "y"
    @test getb_nested_4851(true) == "y"

    # Vector{T} embedded field
    vp = VecParamProbe4851([1, 2, 3])
    @test vp isa VecParamProbe4851{Int64}
    @test typeof(vp) == VecParamProbe4851{Int64}
    @test vp.a == [1, 2, 3]

    # Multiple distinct type parameters inside one tuple field
    pp = PairParamProbe4851((1, "x"))
    @test pp isa PairParamProbe4851{Int64, String}
    @test typeof(pp) == PairParamProbe4851{Int64, String}
    @test pp.a == (1, "x")
end
end # module Agg_nested_param_constructor_4851

# ===== source: types/ntuple_first_class_4973.jl =====
module Agg_ntuple_first_class_4973
# Issue #4973: `ntuple` must be a first-class function value.
#
# Previously `ntuple` existed only as a Rust builtin HOF (BuiltinId::Ntuple),
# so referencing it as a value (`f = ntuple`, `Base.ntuple`) raised
# UndefVarError / "Base has no function named ntuple". It is now backed by a
# pure-Julia method in base/tuple.jl while the direct call shapes
# `ntuple(f, n)` / `ntuple(f, Val(N))` keep their constant-propagation fast path.

using Test

double_idx_4973(i) = 2i

function add_capture_4973(a)
    g = ntuple
    return g(i -> i + a, 3)
end

apply_4973(fn) = fn(identity, 3)
call_function_arg_10423(f, n::Integer) = f(1)
apply_function_arg_10423(fn) = fn(identity, 3)

function indirect_negative_4973()
    f = ntuple
    try
        f(identity, -1)
        return false
    catch e
        return e isa ArgumentError
    end
end

@testset "ntuple first-class (Issue #4973)" begin
    # Direct call shapes still work (fast path preserved).
    @test ntuple(identity, 3) == (1, 2, 3)
    @test ntuple(double_idx_4973, 4) == (2, 4, 6, 8)
    @test ntuple(identity, 0) == ()

    # Val length fast path preserved.
    @test ntuple(identity, Val(3)) == (1, 2, 3)
    @test ntuple(double_idx_4973, Val(4)) == (2, 4, 6, 8)
    @test ntuple(identity, Val(0)) == ()

    # First-class value via local binding.
    f = ntuple
    @test f(identity, 3) == (1, 2, 3)
    @test f(double_idx_4973, 4) == (2, 4, 6, 8)
    @test f(identity, 0) == ()

    # Base-qualified value.
    g = Base.ntuple
    @test g(identity, 3) == (1, 2, 3)

    # Captured closure passed through a first-class ntuple reference.
    @test add_capture_4973(10) == (11, 12, 13)

    # Passing ntuple as an argument to another function.
    @test apply_4973(ntuple) == (1, 2, 3)

    # Issue #10423: runtime specialization must not turn a direct function
    # value argument into a bare module-local global lookup.
    @test apply_function_arg_10423(call_function_arg_10423) == 1

    # First-class invocation validates its length argument like upstream.
    @test indirect_negative_4973()
end
end # module Agg_ntuple_first_class_4973

# ===== source: types/ntuple_val_length_4975.jl =====
module Agg_ntuple_val_length_4975
using Test

# Issue #4975: ntuple(f, Val(N)) must extract the numeric value parameter N
# from the Val{N} struct passed directly as the length argument.

sq_4975(i) = i * i
captured_ntuple_val_4975(a) = ntuple(i -> i + a, Val(3))

@testset "ntuple with Val length (Issue #4975)" begin
    @test ntuple(identity, Val(3)) == (1, 2, 3)
    @test ntuple(i -> i, Val(3)) == (1, 2, 3)
    @test ntuple(identity, Val{3}()) == (1, 2, 3)
    @test ntuple(sq_4975, Val(3)) == (1, 4, 9)
    @test ntuple(i -> i^2, Val(4)) == (1, 4, 9, 16)
    @test ntuple(identity, Val(0)) == ()
    @test ntuple(identity, Val(1)) == (1,)
    @test captured_ntuple_val_4975(10) == (11, 12, 13)
end
end # module Agg_ntuple_val_length_4975

# ===== source: types/val_tuple_value_param_5067.jl =====
module Agg_val_tuple_value_param_5067
# Issue #5067: structured (tuple / nested) value type parameters for Val{...}.
#
# Upstream Julia allows any isbits / Symbol / Tuple value as a type parameter,
# so `Val{(1, 2)}`, `Val{(:a, :b)}`, and `Val{(1, (2, 3))}` are all valid
# DataTypes. typeof renders the tuple parameter with a space after each comma
# (`Val{(1, 2)}`), while a source literal may omit it (`Val{(1,2)}`); upstream
# treats both spellings as the same DataType, so `isa` must ignore that
# cosmetic comma spacing.
using Test

# Dispatch on tuple value parameters.
f(::Val{(1, 2)}) = "onetwo"
f(::Val{(3, 4)}) = "threefour"

# Dispatch on symbol-tuple value parameters.
g(::Val{(:a, :b)}) = "ab"
g(::Val{(:c, :d)}) = "cd"

# Dispatch on a nested-tuple value parameter.
h(::Val{(1, (2, 3))}) = "nested"

@testset "Val tuple/nested value parameters (Issue #5067)" begin
    # typeof display renders ", " between tuple elements, matching upstream.
    @assert string(typeof(Val{(1, 2)}())) == "Val{(1, 2)}"
    @assert string(typeof(Val{(:a, :b)}())) == "Val{(:a, :b)}"
    @assert string(typeof(Val{(1, (2, 3))}())) == "Val{(1, (2, 3))}"

    # isa is insensitive to the cosmetic space after each comma.
    @assert Val{(1, 2)}() isa Val{(1,2)}
    @assert Val{(1,2)}() isa Val{(1, 2)}
    @assert Val{(:a, :b)}() isa Val{(:a,:b)}
    @assert Val{(:a,:b)}() isa Val{(:a, :b)}
    @assert Val{(1, (2, 3))}() isa Val{(1,(2,3))}

    # Distinct tuple parameters are distinct DataTypes.
    @assert !(Val{(1, 2)}() isa Val{(1, 3)})
    @assert !(Val{(:a, :b)}() isa Val{(:a, :c)})

    # Dispatch selects the method whose tuple value parameter matches.
    @assert f(Val{(1, 2)}()) == "onetwo"
    @assert f(Val{(3, 4)}()) == "threefour"
    @assert g(Val{(:a, :b)}()) == "ab"
    @assert g(Val{(:c, :d)}()) == "cd"
    @assert h(Val{(1, (2, 3))}()) == "nested"

    @test true
end
end # module Agg_val_tuple_value_param_5067

# ===== source: types/vararg_tuple_value_param_4841.jl =====
module Agg_vararg_tuple_value_param_4841
# Issue #4841: explicit `Tuple{Vararg{T,N}} where {T,N}` parameter
# signatures must bind both `T` (element type) and `N` (length value
# parameter) via the same dispatch path as the synonymous
# `NTuple{N,T} where {T,N}` alias form. Without the fix, sjulia
# reported `Dispatch(NoMethodFound)` because the `Vararg{T,N}` inner
# was parsed as a bare `JuliaType::Struct("Vararg{T,N}")` and the
# enclosing `Tuple{...}` became a one-element `TupleOf`, so the
# element-wise tuple matcher refused any multi-element call site.
#
# Fix: in `JuliaType::from_name`, translate
# `Tuple{Vararg{T,N}}` into the canonical `NTuple{N,T}` spelling so
# the existing NTuple infrastructure (dispatch matching, val-parameter
# detection in compile/mod.rs, runtime length/type binding in
# vm/mod.rs) picks it up unchanged.

using Test

v_4841(xs::Tuple{Vararg{T,N}}) where {T,N} = (T, N)
w_4841(xs::Tuple{Vararg{T,3}}) where T = T
u_4841(xs::Tuple{Vararg{Int64,N}}) where N = N
h_4841(xs::NTuple{N,T}) where {N,T} = (N, T)

@testset "Tuple{Vararg{T,N}} binds both T and N (Issue #4841)" begin
    @test v_4841((1, 2, 3)) == (Int64, 3)
    @test v_4841((Int32(1), Int32(2))) == (Int32, 2)
    @test v_4841((1.5, 2.5)) == (Float64, 2)
end

@testset "Tuple{Vararg{T,3}} with concrete N binds T (Issue #4841)" begin
    @test w_4841((1, 2, 3)) == Int64
    @test w_4841((1.0, 2.0, 3.0)) == Float64
end

@testset "Tuple{Vararg{Int64,N}} with concrete T binds N (Issue #4841)" begin
    @test u_4841((1, 2, 3)) == 3
    @test u_4841((1, 2)) == 2
    @test u_4841((1,)) == 1
end

@testset "NTuple{N,T} alias form still works (Issue #4841 regression guard)" begin
    # The NTuple{N,T} spelling already worked before #4841; confirm the
    # canonicalization step did not regress it.
    @test h_4841((1, 2, 3)) == (3, Int64)
    @test h_4841((Int32(1), Int32(2))) == (2, Int32)
end
end # module Agg_vararg_tuple_value_param_4841

# ===== prevention: value-parameter load matrix (Issue #10457) =====
module Agg_value_param_load_matrix_10457
using Test

# Issue #10457 (regression: Issue #8869): `LoadTypeBinding` must prefer the
# raw value local stored by bind_type_params for EVERY value-parameter kind
# the VM can bind — Int64, Float64, Bool, Char, Symbol, and Tuple — not just
# an integer-like subset. Before PR #10456 only I64/Bool/Char were raw-loaded,
# so a Symbol-valued parameter fell through to a `DataType(:hello)` wrapper.
# Every getter below shares the same body shape (`= v`), which lowers to a
# bare `LoadTypeBinding`; a kind that falls through to the DataType-wrapper
# path fails its `isa`/`===` assertion or throws in the value-use helpers.

struct VP10457{v} end
vp_get_10457(::VP10457{v}) where {v} = v
# The bound parameter must behave as a plain value inside the method body:
# a DataType wrapper would throw a MethodError in each helper below.
vp_plus1_10457(::VP10457{v}) where {v} = v + 1
# `String(v)` on a where-bound value parameter does not compile yet
# (Issue #10597); the lowercase `string(v)` form covers the same
# raw-Symbol-value use until that gap closes.
vp_string_10457(::VP10457{v}) where {v} = string(v)
vp_first_10457(::VP10457{v}) where {v} = v[1]

@testset "value-parameter load matrix (Issues #10457/#8869)" begin
    # Int64
    @test vp_get_10457(VP10457{42}()) === 42
    @test vp_get_10457(VP10457{42}()) isa Int64
    @test vp_plus1_10457(VP10457{42}()) === 43
    # Float64
    @test vp_get_10457(VP10457{1.5}()) === 1.5
    @test vp_get_10457(VP10457{1.5}()) isa Float64
    @test vp_plus1_10457(VP10457{1.5}()) === 2.5
    # Bool
    @test vp_get_10457(VP10457{true}()) === true
    @test vp_get_10457(VP10457{false}()) === false
    @test vp_get_10457(VP10457{true}()) isa Bool
    # Char
    @test vp_get_10457(VP10457{'q'}()) === 'q'
    @test vp_get_10457(VP10457{'q'}()) isa Char
    # Symbol
    @test vp_get_10457(VP10457{:tag}()) === :tag
    @test vp_get_10457(VP10457{:tag}()) isa Symbol
    @test vp_get_10457(VP10457{:tag}()) !== :other
    @test vp_string_10457(VP10457{:tag}()) == "tag"
    # Tuple (integer / symbol / nested)
    @test vp_get_10457(VP10457{(1, 2)}()) === (1, 2)
    @test vp_get_10457(VP10457{(1, 2)}()) isa Tuple
    @test vp_first_10457(VP10457{(1, 2)}()) === 1
    @test vp_get_10457(VP10457{(:a, :b)}()) === (:a, :b)
    @test vp_first_10457(VP10457{(:a, :b)}()) === :a
    @test vp_get_10457(VP10457{(1, (2, 3))}()) === (1, (2, 3))
    @test vp_first_10457(VP10457{(1, (2, 3))}()) === 1
end
end # module Agg_value_param_load_matrix_10457

# ===== prevention: narrow-numeric value parameters (Issue #10599) =====
module Agg_value_param_narrow_numeric_10599
using Test

# Issue #10599: constructor-form narrow-numeric value type parameters
# (`VP{Int8(5)}`, `VP{UInt8(5)}`, `VP{Float16(1.5)}`) were erased to a
# `DataType` wrapper in generic where-parametric struct-tag methods. The
# runtime struct-construction path (`NewDynamicParametricStruct`) rendered
# the type parameter as `Any` (the inline match fell through for every
# `Value` variant except I64/F64/Bool/Char/Symbol/Tuple), so
# `typeof(g(VP{Int8(5)}()))` reported `DataType` instead of `Int8`. The fix
# renders narrow numerics with the same round-trippable spelling the
# type-value path uses (signed/`Float16` in constructor form, unsigned as
# the hex form Julia displays) so the where-binder recovers the exact value
# AND its narrow type. Each getter lowers to a bare `LoadTypeBinding`; a
# kind that leaks a DataType wrapper fails its `isa`/`===` assertion or
# throws in the arithmetic helper.
#
# NB: the OUTER type display (`typeof(VP{Int8(5)}())`) is intentionally not
# asserted — upstream Julia renders every narrow numeric parameter with the
# lossy short form (`VP{5}`, `VP{0x05}`), which cannot round-trip through
# sjulia's single type-name-string identity model; only the extracted value
# and its type are checked here (both match upstream exactly).

struct VP10599{v} end
vpn_get_10599(::VP10599{v}) where {v} = v
vpn_plus1_10599(::VP10599{v}) where {v} = v + one(typeof(v))
vpn_disp_i8_10599(::VP10599{Int8(5)}) = "i8"
vpn_disp_i64_10599(::VP10599{5}) = "i64"

@testset "narrow-numeric value parameters (Issue #10599)" begin
    # Signed narrow integers round-trip to their exact width.
    @test vpn_get_10599(VP10599{Int8(5)}()) === Int8(5)
    @test vpn_get_10599(VP10599{Int8(5)}()) isa Int8
    @test vpn_plus1_10599(VP10599{Int8(5)}()) === Int8(6)
    @test vpn_get_10599(VP10599{Int16(5)}()) === Int16(5)
    @test vpn_get_10599(VP10599{Int16(5)}()) isa Int16
    @test vpn_get_10599(VP10599{Int32(5)}()) === Int32(5)
    @test vpn_get_10599(VP10599{Int32(5)}()) isa Int32
    @test vpn_get_10599(VP10599{Int128(5)}()) === Int128(5)
    @test vpn_get_10599(VP10599{Int128(5)}()) isa Int128
    # Unsigned narrow integers round-trip from the hex spelling.
    @test vpn_get_10599(VP10599{UInt8(5)}()) === UInt8(5)
    @test vpn_get_10599(VP10599{UInt8(5)}()) isa UInt8
    @test vpn_get_10599(VP10599{UInt16(5)}()) === UInt16(5)
    @test vpn_get_10599(VP10599{UInt16(5)}()) isa UInt16
    @test vpn_get_10599(VP10599{UInt32(5)}()) === UInt32(5)
    @test vpn_get_10599(VP10599{UInt32(5)}()) isa UInt32
    @test vpn_get_10599(VP10599{UInt64(5)}()) === UInt64(5)
    @test vpn_get_10599(VP10599{UInt64(5)}()) isa UInt64
    @test vpn_get_10599(VP10599{UInt128(5)}()) === UInt128(5)
    @test vpn_get_10599(VP10599{UInt128(5)}()) isa UInt128
    # Float16.
    @test vpn_get_10599(VP10599{Float16(1.5)}()) === Float16(1.5)
    @test vpn_get_10599(VP10599{Float16(1.5)}()) isa Float16
    # A typed-integer parameter is a DISTINCT type from the bare-Int64 one and
    # dispatches to its own method (it is not silently widened to Int64).
    @test vpn_disp_i8_10599(VP10599{Int8(5)}()) == "i8"
    @test vpn_disp_i64_10599(VP10599{5}()) == "i64"
    @test Int8(5) !== 5
end
end # module Agg_value_param_narrow_numeric_10599

true
