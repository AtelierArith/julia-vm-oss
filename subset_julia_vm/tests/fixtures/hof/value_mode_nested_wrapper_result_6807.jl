# Issue #6807: the value-mode HOF result builder
# (`hof_exec/value_mode.rs::create_typed_array_from_values`) now emits the
# MemoryRef-backed `Array{T,N}` wrapper instead of the legacy native carrier,
# and no longer materializes nested array-wrapper elements to a native carrier
# (the #5229 leak is prevented by the typeinfo-prefix formatter handling
# wrapper elements, Issue #6882).
#
# A nested `map(x -> map(...), v)` result must therefore still display bare and
# remain indexable/usable. Verified against upstream Julia 1.12.6.

using Test

v = [[1, 2], [3, 4], [5, 6]]

@testset "value_mode_nested_wrapper_result_6807: nested map display + index" begin
    r = map(x -> map(y -> y * 10, x), v)
    @test string(r) == "[[10, 20], [30, 40], [50, 60]]"
    @test r[1] == [10, 20]
    @test r[2][1] == 30
    @test length(r) == 3
end

@testset "value_mode_nested_wrapper_result_6807: map producing fresh arrays" begin
    cols = map(i -> [i, i * 2, i * 3], 1:3)
    @test string(cols) == "[[1, 2, 3], [2, 4, 6], [3, 6, 9]]"
    @test cols[2] == [2, 4, 6]
    total = 0
    for c in cols
        total += c[1]
    end
    @test total == 6
end

@testset "value_mode_nested_wrapper_result_6807: scalar + struct map still work" begin
    @test map(x -> sum(x), v) == [3, 7, 11]
    @test string(map(x -> sum(x), v)) == "[3, 7, 11]"
end

@testset "value_mode_nested_wrapper_result_6807: mutate a mapped result" begin
    r = map(i -> [i], 1:3)
    push!(r[1], 99)
    @test r[1] == [1, 99]
    @test r[2] == [2]
end

true
