# Issue #8561: per-call-site inline caches for dynamic dispatch must be
# invalidated by eval-time method redefinition — after warming a dynamic
# dispatch site in a loop, redefining one of its methods must make the site
# observe the NEW behavior, never a cached pre-redefinition target.
#
# Verified against upstream Julia (julia --startup-file=no): prints 4, 202.

using Test

warmg8561(x::Int) = 1
warmg8561(x::Float64) = 2

function warmloop8561(xs)
    s = 0
    for x in xs
        s += warmg8561(x)
    end
    s
end

xs8561 = Any[1, 2.0, 1]

@testset "eval method redefinition after dynamic-dispatch warm-up (Issue #8561)" begin
    # Warm the dynamic dispatch site (repeatedly, so any call-site cache
    # is certainly filled before the redefinition).
    @test warmloop8561(xs8561) == 4
    @test warmloop8561(xs8561) == 4

    @eval warmg8561(x::Int) = 100

    # The warmed site must observe the redefined method.
    @test warmloop8561(xs8561) == 202
    @test warmg8561(1) == 100
    @test warmg8561(2.0) == 2
end

true
