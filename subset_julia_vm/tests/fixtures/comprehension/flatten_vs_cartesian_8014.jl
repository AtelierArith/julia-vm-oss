# A comprehension with two `for` clauses separated by WHITESPACE (no comma) is a
# flattened (nested) generator and must produce a 1-D `Vector` with
# `Iterators.flatten` semantics. The comma form `for x in A, y in B` is the
# cartesian / multidimensional form and stays an N-D `Array`. sjulia previously
# treated every multi-`for` comprehension as N-D by clause count, so the flatten
# form was wrongly built as a 2-D `Matrix` (Issue #8014).

using Test

@testset "Flatten vs cartesian comprehension (Issue #8014)" begin
    # whitespace flatten form ⇒ 1-D Vector (the issue's MWE)
    flat = [x + y for x in 1:2 for y in 10:10:20]
    @test flat == [11, 21, 12, 22]
    @test flat isa Vector{Int64}
    @test ndims(flat) == 1

    # comma cartesian form ⇒ 2-D Matrix (regression guard)
    grid = [x + y for x in 1:2, y in 10:10:20]
    @test grid == [11 21; 12 22]
    @test grid isa Matrix{Int64}
    @test ndims(grid) == 2

    # three whitespace `for` clauses still flatten to 1-D
    three = [(i, j) for i in 1:2 for j in 1:2 for k in 1:2]
    @test three == [(1, 1), (1, 1), (1, 2), (1, 2), (2, 1), (2, 1), (2, 2), (2, 2)]
    @test ndims(three) == 1

    # dependent inner range: the inner iterator is re-evaluated per outer step
    dep = [(i, j) for i in 1:3 for j in 1:i]
    @test dep == [(1, 1), (2, 1), (2, 2), (3, 1), (3, 2), (3, 3)]

    # mixed: a comma group followed by a whitespace `for` flattens to 1-D, the
    # comma group iterating column-major within the flatten
    mixed = [(i, j, k) for i in 1:2, j in 1:2 for k in 1:2]
    @test mixed == [(1, 1, 1), (1, 1, 2), (2, 1, 1), (2, 1, 2),
                    (1, 2, 1), (1, 2, 2), (2, 2, 1), (2, 2, 2)]
    @test ndims(mixed) == 1

    # filter on a flatten comprehension keeps it 1-D (no spurious reshape)
    filtered = [x + y for x in 1:3 for y in 1:3 if x + y == 4]
    @test filtered == [4, 4, 4]

    # typed flatten comprehension builds a typed 1-D Vector
    typed = Float64[x + y for x in 1:2 for y in 10:10:20]
    @test typed == [11.0, 21.0, 12.0, 22.0]
    @test typed isa Vector{Float64}

    # empty outer range preserves the element type as a 1-D Vector
    empty = [x for x in 1:0 for y in 1:2]
    @test empty == Int64[]
    @test empty isa Vector{Int64}
end

true  # Test passed
