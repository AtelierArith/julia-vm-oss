using Test

# Issue #5774: arrays allocated through the pure-Julia `Array` wrapper path
# (zeros / fill / ones / similar) dropped the upstream `T[...]` typeinfo prefix
# for non-implicit eltypes — only `Bool` was prefixed. `print`/`string`/`println`
# now emit the prefix for non-implicit scalar eltypes (Int8/Float32/ComplexF64/…)
# while implicit eltypes (Int64/Float64/Char/String/Symbol) and composite
# (Tuple) eltypes stay bare, matching `show`.

@testset "wrapper-compact array typeinfo prefix (Issue #5774)" begin
    # Non-implicit scalar eltypes gain the prefix
    @test string(zeros(Int8, 3)) == "Int8[0, 0, 0]"
    @test string(fill(Int16(5), 3)) == "Int16[5, 5, 5]"
    @test string(zeros(Float32, 2)) == "Float32[0.0, 0.0]"
    @test string(fill(1.0f0, 2)) == "Float32[1.0, 1.0]"
    @test string(zeros(ComplexF64, 2)) == "ComplexF64[0.0 + 0.0im, 0.0 + 0.0im]"

    # Implicit eltypes stay bare
    @test string(zeros(Int64, 2)) == "[0, 0]"
    @test string(zeros(2)) == "[0.0, 0.0]"

    # Empty arrays keep their type prefix
    @test string(Int8[]) == "Int8[]"

    # 2D matrix prefix
    @test string(zeros(Int8, 2, 2)) == "Int8[0 0; 0 0]"

    # Bool keeps its 1/0 element rendering + prefix
    @test string(trues(3)) == "Bool[1, 1, 1]"

    # Composite (Tuple) eltype stays bare (homogeneous-implicit), no spurious prefix
    @test string(fill((1, 2), 2)) == "[(1, 2), (1, 2)]"
end

true
