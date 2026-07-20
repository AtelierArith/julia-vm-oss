using Test

# Regression test for Issue #10220, a follow-up to #10078/#10257/#10294.
#
# The existing #10078 fixture (module_struct_bare_name_base_clobber_10078.jl)
# proves compiling Base's own function bodies referencing a clobbered bare
# struct name does not ERROR (`retry(41) == 42` as a proxy) and that the
# module's own struct is still constructible -- but it never actually CALLS
# `Iterators.partition(...)`, so it did not catch #10220: bare
# `Partition(...)` construction *inside* Base's own `partition()` function
# body (subset_julia_vm/src/julia/base/iterators.jl) went through a
# DIFFERENT, non-origin-aware resolution path than the one #10078/#10257
# fixed.
#
# Root cause (Issue #10220, diagnosed precisely in #10294):
# `CoreCompiler::visible_using_modules_for_name`
# (subset_julia_vm_compile/src/compile/core_compiler.rs) answers "which
# `using`-imported modules export this bare name" from `self.usings` /
# `self.resolved_usings`, which are fed from the WHOLE combined program's
# top-level `using` statements (`CorePipeline::usings_set`, sourced from
# `program.usings`) -- Base/prelude's own top level and the USER SCRIPT's own
# top level both have `module_path == None`, so they are indistinguishable
# there, and the SAME flat set reaches every `CoreCompiler` regardless of
# scope. It never checked `self.in_base_function_scope`.
#
# So while compiling Base's OWN `partition()` body, the bare call
# `Partition(itr, Int64(n))` was qualified (via
# `compile/expr/call/mod.rs`'s constructor-resolution chain) to
# `MyPkg.Partition` (2 fields: `n::Int`, `part::Vector`) instead of Base's own
# `Partition` (2 fields: `xs`, `n::Int64`) -- even though the bare,
# origin-aware `lookup_bare_struct_info` path (#10078/#10257) exists and
# would have resolved it correctly. The wrong field order made the runtime
# coerce `itr` (an `Array`) into the `n::Int` slot, raising a `DynamicToI64`
# conversion error for every `Iterators.partition(...)` call after the
# collision.
#
# The fix makes `visible_using_modules_for_name` itself return empty
# immediately when `self.in_base_function_scope` is true, so EVERY caller
# (constructor routing, type-alias resolution, parametric-struct resolution,
# generic call qualification) falls through to the origin-aware bare-name
# path uniformly, plus hardens `CorePipeline::compile_functions`'s
# `function_scope_usings` to not fall back to `program.usings` for a
# Base-origin top-level function (defense in depth).
#
# This fixture actually exercises the iteration protocol end-to-end
# (construction, `iterate`, `collect`, `length`) so a regression in ANY step
# of `Iterators.partition`'s pipeline is caught -- not just "did compilation
# succeed". (A structurally different, deeper variant -- a Base PARAMETRIC
# struct's bare name, e.g. `Enumerate{I}`, is not protected by this fix at
# all, since it never claims a `struct_table` bare slot to begin with -- is
# tracked separately in Issue #10445.)
module MyPkg
export Partition
struct Partition
    n::Int
    part::Vector
end
end
using .MyPkg
using Base.Iterators

# Shadowing the Base keyword-parameter name `retry` forces
# `should_skip_base_cache_for_program` to bypass the frozen Base bytecode
# cache -- the only way this bug is reachable (Issue #10078/#10220).
retry(x) = x + 1

@testset "Base Iterators.partition survives a same-named module struct bare-name clobber (Issue #10220)" begin
    # The module's own struct is still constructible under its bare name.
    p = Partition(2, [10, 20])
    @test p.n == 2
    @test p.part == [10, 20]

    # Base's OWN `Iterators.partition` must still construct, iterate, and
    # report length correctly -- exercising the exact `Partition(itr,
    # Int64(n))` call inside Base's own `partition()` function body that
    # #10220 corrupted.
    q = partition([1, 2, 3, 4, 5], 2)
    chunks = Vector{Int}[]
    for chunk in q
        push!(chunks, collect(chunk))
    end
    @test chunks == [[1, 2], [3, 4], [5]]
    @test length(partition([1, 2, 3, 4, 5], 2)) == 3
end

true
