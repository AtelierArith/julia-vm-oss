# Prevention test: user-defined function coexists with base function of same name (Issue #2726)
# Verifies that exact signature matching preserves cache alignment so that
# user-defined methods do not break base methods.

using Test

struct _PrevTestMinType
    value::Int64
end

function min(x::_PrevTestMinType, y::_PrevTestMinType)
    _PrevTestMinType(x.value < y.value ? x.value : y.value)
end

@testset "user min coexists with base min" begin
    # Base min still works for built-in types
    @test min(1, 2) == 1
    @test min(3.0, 1.0) == 1.0

    # User-defined min works for custom type
    @test min(_PrevTestMinType(5), _PrevTestMinType(3)).value == 3
    @test min(_PrevTestMinType(1), _PrevTestMinType(7)).value == 1
end

true
