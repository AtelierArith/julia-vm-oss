using Test

# Issue #6170 / #5936: @generated functions called through map should run the
# generated body with concrete type objects, not runtime element values.

@generated function generated_map_call_6170(x)
    if x == Int64
        return :(10)
    elseif x == Float64
        return :(20)
    else
        return :(30)
    end
end

@testset "generated map call (Issue #6170)" begin
    @test map(generated_map_call_6170, [1, 2, 3]) == [10, 10, 10]
    @test map(generated_map_call_6170, [1.0, 2.0]) == [20, 20]
end

map(generated_map_call_6170, [1, 2, 3]) == [10, 10, 10] &&
    map(generated_map_call_6170, [1.0, 2.0]) == [20, 20]
