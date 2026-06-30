# Regression test for the array query/construction boundary in
# subset_julia_vm/src/vm/builtins_arrays.rs (Issue #3908). The Similar,
# Reshape, Size, Ndims, Keytype, and Valtype handlers now route their
# `Value::Array(...)` projection through a shared `value_as_array_ref`
# helper. The behavior observed from Julia (shape, ndims, element type, key
# type, value type, Complex-aware similar storage) must remain identical to
# native Julia across Int64, Float64, Bool, Complex{Float64}, and reshaped
# inputs.

using Test

@testset "Array query helpers (Issue #3908)" begin
    @testset "similar preserves element type and shape" begin
        ints = [1, 2, 3, 4]
        s_ints = similar(ints)
        @test eltype(s_ints) === Int64
        @test size(s_ints) == (4,)
        @test length(s_ints) == 4

        floats = [1.0 2.0; 3.0 4.0]
        s_floats = similar(floats)
        @test eltype(s_floats) === Float64
        @test size(s_floats) == (2, 2)
        @test ndims(s_floats) == 2

        bools = Bool[true, false, true]
        s_bools = similar(bools)
        @test eltype(s_bools) === Bool
        @test size(s_bools) == (3,)
    end

    @testset "similar(arr, T, dims) overrides element type and shape" begin
        ints = [1, 2, 3]
        s = similar(ints, Float64, 2, 2)
        @test eltype(s) === Float64
        @test size(s) == (2, 2)
        @test ndims(s) == 2
    end

    @testset "similar on Complex preserves Complex element type" begin
        cs = Array{ComplexF64}(undef, 2)
        s_cs = similar(cs)
        @test eltype(s_cs) === ComplexF64
        @test size(s_cs) == (2,)
        @test length(s_cs) == 2

        s_cs2 = similar(cs, 3, 2)
        @test eltype(s_cs2) === ComplexF64
        @test size(s_cs2) == (3, 2)
        @test ndims(s_cs2) == 2
    end

    @testset "reshape preserves element type and exposes new shape" begin
        xs = collect(1:6)
        m = reshape(xs, 2, 3)
        @test size(m) == (2, 3)
        @test ndims(m) == 2
        @test eltype(m) === Int64

        m2 = reshape(m, 3, 2)
        @test size(m2) == (3, 2)
        @test ndims(m2) == 2
        @test eltype(m2) === Int64
    end

    @testset "size/ndims report logical shape after reshape" begin
        xs = collect(1.0:8.0)
        m = reshape(xs, 2, 4)
        @test size(m) == (2, 4)
        @test size(m, 1) == 2
        @test size(m, 2) == 4
        @test ndims(m) == 2

        # size beyond ndims returns 1 (Julia convention)
        @test size(m, 3) == 1
    end

    @testset "keytype/valtype on arrays" begin
        ints = [10, 20, 30]
        @test keytype(ints) === Int64
        @test valtype(ints) === Int64

        floats = [1.5, 2.5]
        @test keytype(floats) === Int64
        @test valtype(floats) === Float64

        bools = Bool[true, false]
        @test keytype(bools) === Int64
        @test valtype(bools) === Bool

    end

    @testset "ndims of scalar/range/memory is unchanged" begin
        @test ndims(1) == 0
        @test ndims(1.5) == 0
        @test ndims(true) == 0
        @test ndims(1:5) == 1
    end
end

true
