using Test

# Regression test for Issue #10078: `build_struct_tables`
# (subset_julia_vm_compile/src/compile/pipeline_ctx.rs) registers a module struct
# under both its qualified name (`Module.Name`) and its bare (short) name
# "to allow `Point(...)` syntax after `using .MyModule`" -- but the bare-name
# insert unconditionally overwrote an already-registered Base/prelude struct
# of the same bare name (Base's own top-level `struct Partition` backing
# `Iterators.partition`, subset_julia_vm/src/julia/base/iterators.jl).
#
# Normal execution almost never observes this because the frozen,
# precompiled Base bytecode cache is reused (Base's own functions were
# already compiled -- and their `Partition(...)` references already
# correctly bound -- before any user module existed). The bug only surfaces
# when `should_skip_base_cache_for_program` (subset_julia_vm_compile/src/compile/cache.rs)
# bypasses that cache and (re)compiles Base's OWN function bodies in the SAME
# pass as the colliding module, so a bare `Partition(...)` construction or
# `p::Partition` annotation inside Base's own source resolved to the WRONG
# (clobbered) struct, and a later field access failed to compile
# ("Unknown field 'xs' on struct 'Partition'").
#
# The fix threads an origin-aware `base_struct_table` fallback through
# struct-name / type_id resolution (`CoreCompiler::lookup_bare_struct_info`,
# `SharedCompileContext::type_id_to_struct_name`) so Base's OWN function
# bodies always resolve a bare struct name against their own (Base) origin,
# never a same-named module alias registered later in the same compile pass
# -- while ordinary (non-Base) code keeps seeing the module alias as before,
# so a legitimately-shadowing module struct is still constructible under its
# bare name after `using` (Issue #10078 "Suspected Cause": a naive
# "first registration wins" tweak would instead lose the bare-name slot to
# Base for that case).
module MyPkg
export Partition
struct Partition
    n::Int
    part::Vector
end
end
using .MyPkg

# Shadowing the Base keyword-parameter name `retry` forces
# `should_skip_base_cache_for_program` to bypass the frozen Base bytecode
# cache -- the only way this bug is reachable (Issue #10078).
retry(x) = x + 1

@testset "module struct bare-name alias does not clobber Base struct (Issue #10078)" begin
    # Direction (a): compiling Base's OWN function bodies that reference the
    # bare name "Partition" (`IteratorSize`, `IteratorEltype`, `eltype`,
    # `length`, `iterate` in subset_julia_vm/src/julia/base/iterators.jl)
    # must not error, even though `MyPkg.Partition`'s bare-name alias is
    # registered in the SAME compile pass. Merely reaching this line (instead
    # of a "Compilation error") proves Base's own struct resolution was not
    # corrupted by the module alias.
    @test retry(41) == 42

    # Direction (b): the module's OWN struct must still be constructible
    # under its bare name after `using .MyPkg` -- a regression guard against
    # a naive "first registration wins" fix that would instead lose the
    # bare-name slot to Base for this case (Issue #10078 "Suspected Cause").
    p = Partition(2, [10, 20])
    @test p.n == 2
    @test p.part == [10, 20]
end

true
