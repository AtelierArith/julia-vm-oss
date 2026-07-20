using Test

# Issue #10971: an explicit parametric constructor call whose explicit type
# argument is statically known (`R{Int}(x)`) but whose VALUE argument is
# runtime-unknown (`x::Any`) must still select among overloaded braced inner
# constructors at runtime by value signature, binding the SELECTED
# candidate's own `where` binder name(s) into its frame. Previously the
# compiler could only emit a single statically-selected target
# (`CallStaticParametric`) or a bare `CallTypedDispatch` with no
# self-type-binder payload, so this MethodError'd instead of dispatching.

struct R10971{T}
    x::T
    R10971{T}(x::Int) where T = new(x)
    R10971{T}(x::String) where T = new(T(length(x)))
end

make_r10971(x) = R10971{Int}(x)
@test make_r10971(1).x == 1
@test make_r10971("abc").x == 3
@test typeof(make_r10971(1)) === R10971{Int}
@test typeof(make_r10971("abc")) === R10971{Int}

# Candidate binder names may differ per method (`where T` vs `where S`) —
# the per-candidate payload must bind whichever name each selected candidate
# declares, not a single shared name.
struct RDiffBinder10971{T}
    x::T
    RDiffBinder10971{T}(x::Int) where T = new(x)
    RDiffBinder10971{S}(x::String) where S = new(S(length(x)))
end

make_rdiffbinder10971(x) = RDiffBinder10971{Int}(x)
@test make_rdiffbinder10971(1).x == 1
@test make_rdiffbinder10971("abcd").x == 4

# A runtime value that matches neither candidate's value signature still
# raises a MethodError (fail-closed), like upstream.
@test_throws MethodError make_r10971(1.5)

true
