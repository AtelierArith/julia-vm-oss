# Regression test for the array mutation re-push boundary in
# subset_julia_vm/src/vm/exec/array_mutate.rs (Issue #3908). The Zero,
# ArrayPush, ArrayPop, ArrayPushFirst, ArrayPopFirst, ArrayInsert and
# ArrayDeleteAt handlers now route their `Value::Array(...)` construction
# through shared `push_array_ref` / `push_array_value` helpers. The behavior
# observed from Julia (return values, element types, lengths, shapes) must
# remain identical to native Julia for both Int64 and Float64 carriers.

using Test

@testset "Array mutate helpers (Issue #3908)" begin
    @testset "zero(::Array) preserves shape and element type" begin
        ints = [1, 2, 3]
        z_ints = zero(ints)
        @test z_ints == [0, 0, 0]
        @test length(z_ints) == 3
        @test eltype(z_ints) === Int64

        floats = [1.5, 2.5, 3.5]
        z_floats = zero(floats)
        @test z_floats == [0.0, 0.0, 0.0]
        @test length(z_floats) == 3
        @test eltype(z_floats) === Float64
    end

    @testset "push!/pop! round trip" begin
        xs = [1, 2, 3]
        push!(xs, 4)
        push!(xs, 5)
        @test xs == [1, 2, 3, 4, 5]
        @test length(xs) == 5

        last = pop!(xs)
        @test last == 5
        @test xs == [1, 2, 3, 4]
        @test length(xs) == 4
    end

    @testset "pushfirst!/popfirst! round trip" begin
        ys = [10, 20, 30]
        pushfirst!(ys, 5)
        pushfirst!(ys, 0)
        @test ys == [0, 5, 10, 20, 30]
        @test length(ys) == 5

        first_val = popfirst!(ys)
        @test first_val == 0
        @test ys == [5, 10, 20, 30]
        @test length(ys) == 4
    end

    @testset "insert!/deleteat! preserve order" begin
        zs = [1, 2, 4, 5]
        insert!(zs, 3, 3)
        @test zs == [1, 2, 3, 4, 5]
        @test length(zs) == 5

        deleteat!(zs, 1)
        @test zs == [2, 3, 4, 5]
        @test length(zs) == 4

        deleteat!(zs, length(zs))
        @test zs == [2, 3, 4]
        @test length(zs) == 3
    end

    @testset "mutations chain across helpers" begin
        ws = Float64[1.0, 2.0, 3.0]
        push!(ws, 4.0)
        pushfirst!(ws, 0.0)
        insert!(ws, 3, 1.5)
        @test ws == [0.0, 1.0, 1.5, 2.0, 3.0, 4.0]
        @test eltype(ws) === Float64

        deleteat!(ws, 3)
        @test ws == [0.0, 1.0, 2.0, 3.0, 4.0]
        last_val = pop!(ws)
        first_val = popfirst!(ws)
        @test last_val == 4.0
        @test first_val == 0.0
        @test ws == [1.0, 2.0, 3.0]
    end
end

true
