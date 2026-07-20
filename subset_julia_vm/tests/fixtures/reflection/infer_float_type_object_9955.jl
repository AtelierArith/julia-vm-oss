using Test

# Issue #9955: `float(::Type{T}) where {T<:AbstractFloat} = T` returns the
# where-bound type parameter directly. Upstream infers the call's result as
# the precise singleton `Type{Float64}` (constant type-object propagation
# through the method return); sjulia previously widened it to `Any` at
# reflection time even though execution already returned the correct runtime
# value `Float64`.
#
# Root cause: the abstract-interpretation engine's method-table dispatch path
# (`compile/abstract_interp/engine/mod.rs`) used a *static*, call-independent
# `Type{T}` snapshot for such methods without ever binding `T` from the
# concrete call-site argument types, and a bare type-name identifier used as a
# plain value (e.g. `Float64` passed to `float`) inferred as `Any` /
# `Function` instead of the `Type{Float64}` singleton lattice element. Fixed
# by recognizing known type names as `ConcreteType::DataType` values
# (constant type-object propagation, generalizing the `promote_type`-only
# `promote_type_arg_datatype` special case to every call/use site) and
# instantiating a dispatched method's generic `Type{T}` return snapshot from
# the call's concrete argument types. See also Issue #10045 (the broader
# first-class `TypeValue` lattice-element roadmap this is a step toward).
g_float64_9955() = float(Float64)
g_promote_9955() = float(promote_type(Int, Float64))

@testset "Issue #9955 infer float(::Type) type-object result" begin
    # The exact MWE from the issue: both reflection surfaces report the
    # precise `Type{Float64}`, matching upstream Julia.
    @test Base.infer_return_type(g_float64_9955, Tuple{}) === Type{Float64}
    @test Core.Compiler.return_type(g_float64_9955, Tuple{}) === Type{Float64}
    @test g_float64_9955() === Float64

    # Nested type-level call: `promote_type(Int, Float64)` itself infers as
    # the type object `Float64` (already precise, Issue #9914), and `float`
    # applied to that nested result stays precise too. Verified against
    # upstream `julia` first (also `Type{Float64}`).
    @test Base.infer_return_type(g_promote_9955, Tuple{}) === Type{Float64}
    @test g_promote_9955() === Float64

    # Regression guard: reflecting on `float` directly (not through a
    # wrapping function) already worked before this fix and must keep
    # working (Issue #5003-style direct where-bound-typevar-return path).
    @test Base.infer_return_type(float, Tuple{Type{Float64}}) === Type{Float64}
end

true
