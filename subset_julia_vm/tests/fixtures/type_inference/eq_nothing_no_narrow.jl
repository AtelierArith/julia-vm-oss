# Issue #3522: == nothing must not narrow types like === nothing does
# (== is overloadable in Julia, so the narrowing is unsound).
# This fixture verifies that runtime behavior is still correct when comparing
# with `==` against nothing — narrowing in inference would not affect runtime
# semantics, but the bug is that previously narrowing was applied. Here we
# just exercise both code paths to ensure inference does not crash.

function f(x)
    if x == nothing
        return 0
    end
    return 1
end

@assert f(nothing) == 0
@assert f(42) == 1
@assert f("hello") == 1

# === nothing still narrows correctly
function g(x)
    if x === nothing
        return 0
    end
    return 1
end

@assert g(nothing) == 0
@assert g(42) == 1

true
