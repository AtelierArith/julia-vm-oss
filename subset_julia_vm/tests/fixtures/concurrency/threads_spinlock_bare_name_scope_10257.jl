# Regression test for Issue #10257 (follow-up to #10078/PR #10233):
# `CoreCompiler::lookup_bare_struct_info` (subset_julia_vm_compile/src/compile/core_compiler.rs)
# ignored `current_module_path` in the `in_base_function_scope` branch, always
# preferring `base_struct_table` (the preserved pre-clobber TOP-LEVEL entry)
# over the module-qualified name. Consequence: while compiling a Base
# SUBMODULE's own function bodies, a bare struct name that the submodule
# itself defines was rebound to the preserved TOP-LEVEL struct of the same
# bare name instead of the submodule's own.
#
# This is not a hypothetical collision in sjulia's own tree: sjulia exposes
# a top-level `SpinLock` (subset_julia_vm/src/julia/base/lock.jl) IN ADDITION
# to `Threads.SpinLock` (subset_julia_vm/src/julia/base/threads.jl, inside
# `module Threads`) -- sjulia-specific dual exposure with no upstream
# equivalent (upstream only has `Base.Threads.SpinLock`, not a separate
# top-level binding), so this fixture only exercises the upstream-portable
# `Threads.SpinLock` surface; see PR for the sjulia-only top-level collision
# detail. The two struct defs happen to have an identical single
# `locked::Bool` field today, so the bug was latent (no fixture observed
# wrong FIELD LAYOUT), but `lock`/`unlock`/`trylock`/`islocked` inside
# `module Threads` must still resolve their own `l::SpinLock` annotation to
# `Threads.SpinLock` -- not silently to the colliding top-level `SpinLock`
# -- or a future divergence in either struct's fields would silently
# miscompile.
#
# Shadowing the Base keyword-parameter name `retry` forces
# `should_skip_base_cache_for_program` (subset_julia_vm_compile/src/compile/cache.rs)
# to bypass the frozen Base bytecode cache and recompile Base's (and its
# submodules') own function bodies fresh in this same pass -- the only way
# this bug is reachable.
using Test

retry(x) = x + 1

@testset "Base submodule bare struct name resolves to its own type (Issue #10257)" begin
    @test retry(41) == 42

    # Threads' OWN SpinLock, constructed and locked/unlocked through Threads'
    # OWN function bodies (in_base_function_scope=true,
    # current_module_path=Some("Threads")) -- must resolve `l::SpinLock`
    # to Threads.SpinLock, not sjulia's colliding top-level SpinLock.
    tl = Threads.SpinLock()
    @test islocked(tl) == false
    lock(tl)
    @test islocked(tl) == true
    unlock(tl)
    @test islocked(tl) == false
end

true
