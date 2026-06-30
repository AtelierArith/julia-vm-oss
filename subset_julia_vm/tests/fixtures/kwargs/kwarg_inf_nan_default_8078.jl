# Issue #8078: a keyword-argument default value of `Inf` (or `-Inf`, `NaN`,
# `Inf32`, ...) was miscompiled to `0` on the no-JIT VM.
#
# Root cause: `Inf`/`NaN` (and the `Inf32`/`Inf16`/`Inf64`/`NaN*` family) are
# Base global *constants* the compiler emits as float literals in expression
# position, but they are not bound runtime globals. The kwarg-default
# evaluators (the baked-constant `eval_literal_default` and the runtime
# mini-interpreter `value_from_bound_name`) looked the bare name up as a bound
# slot/global, missed, and fell through to the `Value::I64(0)` fallback — so an
# omitted-keyword call returned `0`. Positional defaults (which compile the
# default expression directly) were unaffected.
#
# Fix: a shared `float_special_constant_value` resolver maps these names to
# their `Value` in both kwarg-default evaluators (bound names still take
# precedence, so a parameter shadowing the name wins).
#
# Verified against upstream Julia 1.12.6 before implementation.

using Test

# --- the reported MWE --------------------------------------------------------
g_inf_8078(; a = Inf) = a

@testset "kwargs_inf_nan_default_8078: Inf keyword default" begin
    @test g_inf_8078() === Inf
    @test g_inf_8078() isa Float64
end

# --- the Inf / NaN family as keyword defaults --------------------------------
neg_inf_8078(; a = -Inf) = a
nan_8078(; a = NaN) = a
inf32_8078(; a = Inf32) = a
nan32_8078(; a = NaN32) = a
inf16_8078(; a = Inf16) = a
nan16_8078(; a = NaN16) = a
inf64_8078(; a = Inf64) = a
nan64_8078(; a = NaN64) = a

@testset "kwargs_inf_nan_default_8078: Inf/NaN family keyword defaults" begin
    @test neg_inf_8078() === -Inf
    @test isnan(nan_8078())
    @test inf32_8078() === Inf32
    @test isnan(nan32_8078())
    @test inf16_8078() === Inf16
    @test isnan(nan16_8078())
    @test inf64_8078() === Inf
    @test isnan(nan64_8078())
end

# --- types are preserved -----------------------------------------------------
@testset "kwargs_inf_nan_default_8078: default types preserved" begin
    @test typeof(g_inf_8078()) === Float64
    @test typeof(inf32_8078()) === Float32
    @test typeof(inf16_8078()) === Float16
    @test typeof(nan32_8078()) === Float32
end

# --- unary minus over a typed infinity ---------------------------------------
# (A unary-minus default also exercises the Issue #8109 fix: `infer_default_type`
# now recurses through `-` so the slot keeps its float type instead of Int64.)
neg_inf32_8078(; a = -Inf32) = a
neg_inf16_8078(; a = -Inf16) = a

@testset "kwargs_inf_nan_default_8078: negated typed infinity" begin
    @test neg_inf32_8078() === -Inf32
    @test typeof(neg_inf32_8078()) === Float32
    @test neg_inf16_8078() === -Inf16
    @test typeof(neg_inf16_8078()) === Float16
end

# --- annotated optional keyword argument -------------------------------------
ann_inf_8078(; a::Float64 = Inf) = a
ann_inf32_8078(; a::Float32 = Inf32) = a

@testset "kwargs_inf_nan_default_8078: annotated optional kwarg" begin
    @test ann_inf_8078() === Inf
    @test ann_inf32_8078() === Inf32
    @test typeof(ann_inf32_8078()) === Float32
end

# --- the default round-trips through a forwarded keyword ----------------------
inner_inf_8078(; a = Inf) = a
outer_inf_8078(; a = Inf) = inner_inf_8078(; a = a)

@testset "kwargs_inf_nan_default_8078: forwarded Inf keyword default" begin
    @test outer_inf_8078() === Inf
end

# --- @kwdef struct field default ---------------------------------------------
# The `lo = -Inf` field additionally regresses Issue #8109 (a unary-minus
# default mis-inferred Int64, breaking the @kwdef inner-constructor dispatch).
Base.@kwdef struct Bounds8078
    lo::Float64 = -Inf
    hi::Float64 = Inf
end

@testset "kwargs_inf_nan_default_8078: @kwdef Inf field defaults" begin
    @test Bounds8078().hi === Inf
    @test Bounds8078().lo === -Inf
    @test Bounds8078(lo = 0.0).lo === 0.0
end

# --- controls: explicit override + a shadowing parameter still win -----------
ctrl_8078(; a = Inf) = a
# A positional parameter literally named `Inf` must shadow the constant.
shadow_8078(Inf; a = Inf) = a

@testset "kwargs_inf_nan_default_8078: controls" begin
    @test ctrl_8078(a = 3.5) === 3.5
    @test shadow_8078(7) == 7
end

true
