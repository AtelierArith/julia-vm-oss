# Test maxintfloat Pure Julia dispatch (Issue #3732)
#
# After the migration, public `maxintfloat` is routed through the Pure
# Julia method table (base/floatfuncs.jl) rather than a Rust builtin.
# This fixture exercises:
#   - the no-arg form
#   - the `::Type{Float64}` form
#   - the value form `maxintfloat(::Float64)`
#   - calls via a user-defined wrapper (method dispatch path)
#   - calls via function-variable forwarding

using Test

call_no_arg(f) = f()
apply1(f, x) = f(x)
maxintfloat_via_wrapper() = maxintfloat()

@testset "Pure Julia dispatch for maxintfloat (Issue #3732)" begin
    # No-arg form
    @test (maxintfloat()) == 9007199254740992.0

    # Type form — Pure Julia method matches ::Type{Float64}
    @test (maxintfloat(Float64)) == 9007199254740992.0

    # Value form — Pure Julia method matches ::Float64
    @test (maxintfloat(1.0)) == 9007199254740992.0

    # Wrapper / method-dispatch path
    @test (maxintfloat_via_wrapper()) == 9007199254740992.0

    # Function-variable / first-class function path. This used to be
    # shadowed by `BuiltinId::Maxintfloat`; with the migration the call
    # must dispatch through the Pure Julia method.
    @test (call_no_arg(maxintfloat)) == 9007199254740992.0
    @test (apply1(maxintfloat, 1.0)) == 9007199254740992.0
end

true
