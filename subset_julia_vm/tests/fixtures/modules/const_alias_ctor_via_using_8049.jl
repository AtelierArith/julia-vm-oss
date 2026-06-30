# A `const`-bound type alias re-exported from a module and brought into scope
# via `using .B` must be callable as a constructor, not just readable as a
# value (Issue #8049). Previously `Foo()` raised `UndefVarError: Foo not
# defined` because the function-call name-resolution path consulted only the
# callable/function table for the calling scope, which does not include the
# `const`-bound type alias imported via `using`. The value-reference path
# (`t = Foo`) already resolved it. The fix routes a call whose target name is a
# visible type (struct / parametric struct / type alias) to the same
# constructor-resolution chain, matching upstream Julia.

using Test

module A8049
struct Foo
    x
    Foo() = new(42)
end
export Foo
end

module B8049
import ..A8049
const Foo = A8049.Foo
export Foo
end

using .B8049

# Parametric struct alias re-exported via a second module.
module A8049P
struct Pt{T}
    x::T
    y::T
end
export Pt
end

module B8049P
import ..A8049P
const Pt = A8049P.Pt
export Pt
end

using .B8049P

# Selective `using .B: name` of the const alias.
module A8049S
struct Baz
    v
    Baz() = new(99)
end
export Baz
end

module B8049S
import ..A8049S
const Baz = A8049S.Baz
export Baz
end

using .B8049S: Baz

@testset "const type alias exported via using as constructor (Issue #8049)" begin
    # Read of the aliased name resolves to the underlying type.
    t = Foo
    @test t === A8049.Foo

    # Call via a local variable holding the type works.
    @test t().x == 42

    # Direct constructor call on the imported const alias (the regression).
    @test Foo().x == 42
    @test Foo() isa A8049.Foo

    # Direct constructor call on a parametric-struct const alias.
    p = Pt(1.0, 2.0)
    @test p isa A8049P.Pt
    @test p.x == 1.0 && p.y == 2.0

    # Selective `using .B: Baz` const alias is also callable as a constructor.
    @test Baz().v == 99
    @test Baz() isa A8049S.Baz
end

true
