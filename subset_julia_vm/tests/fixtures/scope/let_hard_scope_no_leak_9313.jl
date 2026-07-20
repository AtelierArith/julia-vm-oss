# Issue #9313: a hard-scope `let` is a hard LOCAL scope. A name bound in its body
# (a `let` binding or a body assignment) must be discarded at block exit and must
# NOT leak into the enclosing/module global scope. sjulia runs a top-level `let`
# body in the module frame with global slots, so the binding used to leak and
# `@isdefined` reported it after the `let`. Upstream isolates it; verified at
# parity with julia 1.12.
#
# `@isdefined` results are captured into locals before asserting to avoid nesting
# `@isdefined` inside `@assert` (a separate sjulia macro-in-macro-arg gap).

# Bare `let`: a body assignment does not leak.
let
    leaked = 42
    @assert leaked == 42
end
leaked_defined = @isdefined(leaked)
@assert !leaked_defined "bare let-body local leaked into module scope"

# `let x = 1`: neither the binding nor a body local leaks.
let x = 1
    y = x + 41
    @assert y == 42
end
x_defined = @isdefined(x)
y_defined = @isdefined(y)
@assert !x_defined "let binding leaked into module scope"
@assert !y_defined "let-body local leaked into module scope"

# Nested lets: inner and outer locals are both discarded.
let
    a = 1
    let
        b = 2
        @assert a + b == 3
    end
    b_defined = @isdefined(b)
    @assert !b_defined "inner let local leaked to outer scope"
end
a_defined = @isdefined(a)
@assert !a_defined "outer let local leaked into module scope"

# Shadowing a pre-existing global restores it (the shadow does not leak, and the
# global is not wrongly discarded).
g = 100
let
    g = 5
    @assert g == 5
end
@assert g == 100
g_defined = @isdefined(g)
@assert g_defined "shadowed pre-existing global was wrongly discarded"

# `global` inside a `let` binds the module global and DOES persist.
let
    global keep = 7
end
keep_defined = @isdefined(keep)
@assert keep_defined "global declared in a let was wrongly discarded"
@assert keep == 7

# A closure returned from a value-position `let` captures the let-local by value,
# so it keeps working after the let-local is discarded from the frame.
f = let
    c = 10
    () -> c * 2
end
@assert f() == 20
c_defined = @isdefined(c)
@assert !c_defined "value-position let-local leaked into module scope"

true
