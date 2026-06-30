# Names referenced inside an inner constructor body must resolve in the struct's
# DEFINING module, not in the caller's scope (Issue #8069). Upstream Julia always
# evaluates a method body's names in its definition module, so a module-private
# (non-exported) function, const, or type used inside an inner constructor is
# visible even though the caller never does `using .M`.
#
# Previously `M.UR(...)` raised a compile error ("function 'helper' is not
# imported. Use 'using ModuleName' ...") because the inner-constructor body was
# compiled with the top-level import scope instead of the struct's module scope.
# The fix threads the defining module into inner-constructor compilation, exactly
# as ordinary module method bodies are compiled.

using Test

module M8069

struct Elem end

# Module-private (non-exported) helpers, const, and type.
helper() = 7
combine(a, b) = a + b
const K = 99

# Mutable struct exercising a module-private call in the body, then `new`.
mutable struct UR
    x
    y
    z
    t
    function UR(v)
        h = helper()        # module-private function call in ctor body
        s = combine(v, K)   # module-private function + module-private const
        ty = Elem           # module-private type referenced as a value
        return new(v, h, s, ty)
    end
end

# Immutable struct exercising a module-private call directly inside `new`.
struct Wrap
    a
    Wrap() = new(helper())
end

end # module M8069

# The caller deliberately does NOT `using .M8069`; only qualified access is used.
@testset "inner constructor body resolves names in defining module (Issue #8069)" begin
    r = M8069.UR(5)
    @test r.x == 5            # constructor argument
    @test r.y == 7            # module-private function call result
    @test r.z == 104          # module-private function over (arg, module-private const)
    @test r.t === M8069.Elem  # module-private type referenced as a value

    @test M8069.Wrap().a == 7 # module-private call directly inside new(...)
end

true
