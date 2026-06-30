# Pure Julia dispatch verification for mutating Set methods (Issue #3739)
#
# After removing `push!`, `pop!`, `delete!` from `map_builtin_name()`, public
# calls go through method dispatch first. Pure Julia methods on `Set`
# (`base/set.jl`) become reachable, and a user `Base.push!(::Set, x)` override
# now wins over the Rust intrinsic. Array `push!`/`pop!` keeps using the Rust
# builtin via the explicit `compile_call` route, preserving in-place semantics.

using Test

@testset "Set push!/delete!/empty! reach Pure Julia (return value form)" begin
    s = Set([1, 2, 3])
    pushed = push!(s, 4)
    @test 4 in pushed
    @test length(pushed) == 4

    deleted = delete!(pushed, 2)
    @test !(2 in deleted)

    emptied = empty!(deleted)
    @test length(emptied) == 0
end

@testset "Array push! / pop! still routes through Rust builtin (in-place)" begin
    arr = [1, 2, 3]
    push!(arr, 4)
    @test arr == [1, 2, 3, 4]
    v = pop!(arr)
    @test v == 4
    @test arr == [1, 2, 3]
end

@testset "Char-typed array push! works inside loops (regression for Issue #3739)" begin
    s = "abc"
    chars = Char[]
    for c in s
        push!(chars, c)
    end
    @test length(chars) == 3
end

# Note: a `Base.push!(::Set, x)` override would recurse infinitely on real Julia
# (since the override IS the only matching method) but in SubsetJuliaVM Pure
# Julia push!(::Set, x) eventually calls `_set_push!` which is a Rust intrinsic.
# The override-detection test for Set algebra lives in `sets/test_pure_julia_dispatch.jl`
# (`union(::Set, ::Set)` override) so we don't need to repeat it here.
@testset "Set push! returns updated Set (Pure Julia path)" begin
    s = Set([1, 2])
    pushed = push!(s, 99)
    @test 99 in pushed
    @test length(pushed) == 3
end

true
