using Test

function check_indexin_union_result(a, b, expected)
    r = indexin(a, b)
    ok = typeof(r) === Vector{Union{Nothing, Int64}}
    ok = ok && eltype(r) === Union{Nothing, Int64}
    ok = ok && length(r) == length(expected)
    for i in 1:length(expected)
        if expected[i] === nothing
            ok = ok && r[i] === nothing
        else
            ok = ok && r[i] == expected[i]
        end
    end
    ok
end

@testset "indexin Union{Nothing, Int64} result type (Issues #4018/#4657)" begin
    @test check_indexin_union_result([1, 3], [1, 2], Any[1, nothing])
    @test check_indexin_union_result(Int8[1, 3], Int8[1, 2], Any[1, nothing])
    @test check_indexin_union_result(String["a", "c"], String["a", "b"], Any[1, nothing])
    @test check_indexin_union_result(Any["a", 1], Any[1, "a"], Any[2, 1])
end

true
