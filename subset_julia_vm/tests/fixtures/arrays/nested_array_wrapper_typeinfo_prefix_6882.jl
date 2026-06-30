# Issue #6882: a Vector whose elements are Memory-backed `Array{T,N}` *wrappers*
# (built via the typed `T[...]` / `T[]` forms) must print the bare `[...]` form
# when the inner element type is implicit (`Int64`/`Float64`/`Char`/`String`/
# `Symbol`), not a spurious `Array{T, N}[...]` typeinfo prefix. The native-array
# carrier and the wrapper representation must display identically.
#
# Verified against upstream Julia 1.12.6.

using Test

@testset "nested_array_wrapper_typeinfo_prefix_6882: typed-form inner arrays" begin
    @test string([Int[1], Int[2]]) == "[[1], [2]]"
    @test string([Int[], Int[]]) == "[Int64[], Int64[]]"
    @test string([Float64[1.0], Float64[2.0]]) == "[[1.0], [2.0]]"
    @test string([Int[1, 2], Int[3, 4]]) == "[[1, 2], [3, 4]]"
end

@testset "nested_array_wrapper_typeinfo_prefix_6882: mixed plain + typed" begin
    @test string([[1], Int[]]) == "[[1], Int64[]]"
    @test string([[1, 2], Int[]]) == "[[1, 2], Int64[]]"
end

@testset "nested_array_wrapper_typeinfo_prefix_6882: plain literals still bare" begin
    @test string([[1, 2], [3, 4]]) == "[[1, 2], [3, 4]]"
    @test string([[1.0, 2.0], [3.0, 4.0]]) == "[[1.0, 2.0], [3.0, 4.0]]"
end

# Note: a *non-implicit* inner eltype (e.g. `[Int8[1], Int8[2]]`) is left for a
# follow-up. After this fix the outer prefix is correct (`Vector{Int8}[...]`),
# but sjulia does not yet propagate the typeinfo context into nested element
# formatting, so the inner arrays still print `Int8[1]` instead of the bare `[1]`
# upstream emits under the propagated `Vector{Int8}` typeinfo. That nested
# typeinfo propagation is a separate, deeper formatter change.

true
