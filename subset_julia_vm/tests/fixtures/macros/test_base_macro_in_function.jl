# Prevention test: Base macros work inside function bodies (Issue #2604)
# The no-context lowering path (lower_macro_expr) must handle Base macros
# that the context path (lower_macro_expr_with_ctx) handles.
# If the paths diverge, macros will work at top-level but fail inside functions.

using Test

# @inbounds inside a function body (no-context path)
function sum_inbounds(arr)
    s = 0
    @inbounds for i in 1:length(arr)
        s = s + arr[i]
    end
    s
end

# @simd inside a function body (no-context path)
function sum_simd(arr)
    s = 0
    @simd for i in 1:length(arr)
        s = s + arr[i]
    end
    s
end

# @assert inside a function body (no-context path)
function checked_add(a, b)
    @assert a >= 0
    a + b
end

@testset "base macros in function bodies" begin
    @test sum_inbounds([1, 2, 3]) == 6
    @test sum_simd([1, 2, 3]) == 6
    @test checked_add(1, 2) == 3
end

true
