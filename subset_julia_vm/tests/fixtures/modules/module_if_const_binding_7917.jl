# Module top-level `if`/`elseif`/`else` const assignments create module bindings
# (Issue #7917). `if` introduces no new scope at module top level, so a
# `const`/`global` assignment in any branch is registered as a member of the
# module, exactly as in upstream Julia. This shape is produced by
# AbstractAlgebra's `@alias` macro, which expands to
# `if isdefined(...) ... else const alias = real end` in the module body.

using Test

module M7917
# `const` in a plain `if true` branch.
if true
    const x = 1
end

# `const` selected via an `elseif` chain (each branch is at module scope).
v = 2
if v == 1
    const y = 10
elseif v == 2
    const y = 20
else
    const y = 30
end

# `const` in the `else` branch (the AbstractAlgebra `@alias`-style shape).
have_real = false
if have_real
    const z = 100
else
    const z = 200
end
end

@testset "Module if-block const bindings (Issue #7917)" begin
    @test isdefined(M7917, :x)
    @test M7917.x == 1
    @test M7917.y == 20
    @test M7917.z == 200
end

true
