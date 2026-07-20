# Issue #7729: a module-qualified constructor call `M.Num(x)` must reach the
# synthesized field-count default constructor even when the struct ALSO has a
# user-defined OUTER constructor (which registers `Num` as a module function and
# routes the qualified call through the constructor method table). The declared
# outer constructor `Num(x::Num) = x` does NOT match a `Sym` argument; dispatch
# must fall back to the field-count default ctor instead of erroring with
# NoMethodFound. This is the `has_func == true` sibling of #7631.
#
# Before the fix, `Sym7729.Num(s)` errored with
#   Dispatch(NoMethodFound { name: "Sym7729.Num", arg_types: [Struct("Sym7729.Sym")] })
#
# All references stay module-qualified so the fixture matches upstream Julia
# exactly: `using .Sym7729` does not bring the unexported names into scope, so a
# bare `Num(...)` would be an UndefVarError under Julia. Type identity is probed
# through module-internal helpers (`isnum`) to avoid unrelated qualified-type
# reference limitations.
using Test

module Sym7729
    struct Sym; name; end
    struct Num; val; end
    Num(x::Num) = x          # outer ctor: registers Num as a module function
    isnum(x) = x isa Num     # module-internal type probe
end
using .Sym7729

@testset "Issue #7729: qualified ctor reaches field-count default ctor" begin
    s = Sym7729.Sym(:x)
    # Qualified call must hit the field-count default constructor, not error.
    a = Sym7729.Num(s)
    @test Sym7729.isnum(a)
    @test a.val === s
    # The declared outer constructor still wins for a Num argument (idempotent).
    @test Sym7729.Num(a) === a
end

true
