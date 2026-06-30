# Parametric struct constructor inference must not pick an arbitrary instantiation
# Issue #3534: HashMap iteration order can return the wrong type_id for parametric structs
# when multiple instantiations of the same base struct exist.

using Test

struct Box3534{T}
    value::T
end

function f3534()
    a = Box3534{Int64}(1)
    b = Box3534{String}("x")
    return (a.value, b.value)
end

@testset "Parametric struct distinct instantiations" begin
    result = f3534()
    @test result == (1, "x")
    @test result[1] == 1
    @test result[2] == "x"
end

true
