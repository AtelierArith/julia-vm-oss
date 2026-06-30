# Issue #7575: a child module's unqualified call to a function it defines itself
# must dispatch to its OWN method, not to a same-named, more-specific *typed*
# method in the parent module. sjulia pools dispatch candidates by bare name
# across module boundaries; before the fix the parent's `f(::Number)` won the
# unqualified `f(x)` made from inside the child, so `A.B.g(1)` returned `:outer`.

using Test

module A
f(x::Number) = :outer
module B
f(x) = :inner
g(x) = f(x)
end
end

# A child that does NOT redefine `h` legitimately shares the parent's binding
# through an explicit import — that must keep working (no regression).
module C
h(x::Number) = :parent_h
module D
import ..C: h
h(x::String) = :child_h
# Unqualified call inside D dispatches across BOTH imported parent methods and
# the child's own method (same generic function), so an Int still hits the
# parent typed method.
which_h(x) = h(x)
end
end

@testset "child module unqualified method wins over parent typed method (Issue #7575)" begin
    # The crux of #7575: unqualified `f(x)` inside `A.B.g` is `A.B.f`, not `A.f`.
    @test A.B.g(1) == :inner

    # Direct qualified access stays correct.
    @test A.B.f(1) == :inner
    @test A.f(1) == :outer

    # Explicit `import ..C: h` makes `h` one shared generic function, so cross
    # module dispatch still pools both methods (regression guard).
    @test C.D.which_h(1) == :parent_h
    @test C.D.which_h("x") == :child_h
end

true
