using Test

# Prevention fixture for Issue #10457 (regression: Issue #10423, fixed in PR
# #10456). Both runtime-specialization entry points must never recompile a
# callee body that materializes a resolved first-class function value
# (`PushResolvedFunction`) while an argument is only known as
# `ValueType::Function`: the specializer used to turn the resolved value into
# a bare module-local global lookup, losing its candidate-method metadata.
# The supported behavior is a clean fallback to the generic bytecode route
# (`runtime_specialization_supported_for_function` in
# `subset_julia_vm_vm/src/vm/exec/call.rs`), which guards
# - the `CallSpecialize` instruction path (untyped-param callees), and
# - the direct-call runtime-specialization bridge
#   (`try_specialized_entry_for_runtime_call`, `where`-parametric callees).

# First-class function consumers passed INTO the specialized callees.
# `n::Integer` gives the second param a runtime-open annotation.
call_fn_arg_10457(f, n::Integer) = f(n)
call_dispatch_10457(f, x) = f(x)

# Untyped-param callee -> its call sites compile to `CallSpecialize`; the body
# materializes `identity` via `PushResolvedFunction`.
apply_identity_10457(fn) = fn(identity, 3)

# Same body shape with a different resolved Base function value.
apply_abs_10457(fn) = fn(abs, -7)

# `where`-parametric callee -> its direct call sites take the
# `try_specialized_entry_for_runtime_call` bridge (Issue #6868) instead of a
# `CallSpecialize` site; the body materializes `identity` the same way.
apply_where_10457(fn, x::T) where {T} = fn(identity, x)

# The resolved value must keep its candidate-method metadata: dispatch on the
# passed-through function value still selects among multiple methods.
pick_10457(x::Int64) = :int
pick_10457(x::Float64) = :float
apply_pick_10457(fn, x) = fn(pick_10457, x)

@testset "specializer preserves PushResolvedFunction (Issues #10457/#10423)" begin
    # Repeated calls exercise the initial specialize attempt AND the
    # cached / negative-cached re-entry (specialization happens on the first
    # call at a CallSpecialize site; there is no warmup threshold).
    for _ in 1:5
        @test apply_identity_10457(call_fn_arg_10457) == 3
        @test apply_abs_10457(call_fn_arg_10457) == 7
    end

    # A resolved Base function arriving as the first-class argument itself.
    @test apply_identity_10457(ntuple) == (1, 2, 3)

    # where-parametric direct-call specialization bridge, over several
    # concrete signatures (each is a distinct SpecializationKey).
    for x in (1, 2, 3)
        @test apply_where_10457(call_fn_arg_10457, x) == x
    end
    @test apply_where_10457(call_dispatch_10457, 2.5) == 2.5
    @test apply_where_10457(call_dispatch_10457, :sym) == :sym

    # Candidate-method metadata survives the trip through the callee body.
    @test apply_pick_10457(call_dispatch_10457, 1) == :int
    @test apply_pick_10457(call_dispatch_10457, 1.0) == :float
end

true
