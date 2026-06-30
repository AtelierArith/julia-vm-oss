using Test

function typeof_then_codegen_narrowing_5077(x::Union{Int64,String})
    if typeof(x) === Int64
        return x + 1
    else
        return length(x)
    end
end

function typeof_reversed_codegen_narrowing_5077(x::Union{Int64,String})
    if Int64 == typeof(x)
        return x + 2
    else
        return length(x) + 10
    end
end

function typeof_not_else_codegen_narrowing_5077(x::Union{Int64,String})
    if typeof(x) !== Int64
        return length(x)
    else
        return x + 3
    end
end

@testset "typeof guard codegen narrowing (Issue #5077)" begin
    @test typeof_then_codegen_narrowing_5077(41) == 42
    @test typeof_then_codegen_narrowing_5077("abcd") == 4
    @test typeof_reversed_codegen_narrowing_5077(40) == 42
    @test typeof_reversed_codegen_narrowing_5077("abc") == 13
    @test typeof_not_else_codegen_narrowing_5077(39) == 42
    @test typeof_not_else_codegen_narrowing_5077("abcd") == 4
end

true
