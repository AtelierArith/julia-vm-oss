# Call-site lambda/operator return-type inference for the remaining HOFs after
# migrating them onto the tfuncs registry path via the HofLambdaAnalyzer seam
# (Issue #6604): broadcast (binary/n-ary), filter, reduce/foldl/foldr, and
# mapreduce/mapfoldl/mapfoldr.
#
# The registry rules `tfuncs::hof_ops::{nary_map,filter,reduce,mapreduce}_call_result`
# receive the callable expression through the analyzer seam and call back into
# `CoreCompiler` to infer element/result types. These `typeof` assertions pin
# that the migrated paths produce the same concrete types as before — including
# the reduce-specific operator coverage (`^`) that the binary-map rule lacks.

using Test

@testset "hof_remaining_registry_inference_6604" begin
    # --- binary broadcast: lambda and named operator ---
    b1 = broadcast((x, y) -> x + y * 1.0, [1, 2, 3], [4, 5, 6])
    @test b1 == [5.0, 7.0, 9.0]
    @test typeof(b1) === Vector{Float64}

    b2 = broadcast(+, [1, 2, 3], [4, 5, 6])
    @test b2 == [5, 7, 9]
    @test typeof(b2) === Vector{Int64}

    # --- n-ary map (3 collections) ---
    n1 = map((x, y, z) -> x + y + z + 0.0, [1, 2, 3], [4, 5, 6], [7, 8, 9])
    @test n1 == [12.0, 15.0, 18.0]
    @test typeof(n1) === Vector{Float64}

    # --- filter: element type preserved, predicate type irrelevant ---
    f1 = filter(x -> x > 2, [1, 2, 3, 4])
    @test f1 == [3, 4]
    @test typeof(f1) === Vector{Int64}

    f2 = filter(x -> x > 1.5, [1.0, 2.0, 3.0])
    @test f2 == [2.0, 3.0]
    @test typeof(f2) === Vector{Float64}

    # --- reduce / foldl / foldr ---
    r1 = reduce(+, [1, 2, 3, 4])
    @test r1 == 10
    @test typeof(r1) === Int64

    r2 = reduce((a, b) -> a + b, [1, 2, 3, 4])
    @test r2 == 10
    @test typeof(r2) === Int64

    r3 = reduce(+, [1.0, 2.0, 3.0])
    @test r3 == 6.0
    @test typeof(r3) === Float64

    # `^` is covered by the reduce-result rule but not the binary-map rule.
    r4 = foldl(^, [2, 2, 3])
    @test r4 == 64
    @test typeof(r4) === Int64

    r5 = foldr(-, [1, 2, 3, 4])
    @test r5 == -2
    @test typeof(r5) === Int64

    # --- mapreduce / mapfoldl / mapfoldr ---
    m1 = mapreduce(abs, +, [-1, -2, 3])
    @test m1 == 6
    @test typeof(m1) === Int64

    m2 = mapreduce(x -> x * 1.0, +, [1, 2, 3])
    @test m2 == 6.0
    @test typeof(m2) === Float64

    m3 = mapfoldl(x -> x + 1, *, [1, 2, 3])
    @test m3 == 24
    @test typeof(m3) === Int64

    m4 = mapfoldr(identity, -, [1, 2, 3, 4])
    @test m4 == -2
    @test typeof(m4) === Int64
end

true
