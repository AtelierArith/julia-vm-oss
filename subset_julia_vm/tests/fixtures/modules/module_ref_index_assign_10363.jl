# Issue #10363: `R[] = v` through a module-scope global Ref silently coerced an
# Int64 value to Float64. The compiler's index-assign lowering applied its
# legacy unboxed-F64-array coercion whenever the target's static type was
# unknown — but a zero-index store targets a Ref cell, which stores the value
# verbatim, so the coercion corrupted the stored type. Main-level and
# function-local Refs were unaffected (their static type is known).

using Test

module RefStore10363
R = Ref(0)
R[] = 5
S = Ref(1.5)
S[] = 2.5
end

@testset "module-scope Ref index-assign preserves value type (Issue #10363)" begin
    @test typeof(RefStore10363.R) === Base.RefValue{Int64}
    @test RefStore10363.R[] === 5
    @test typeof(RefStore10363.S) === Base.RefValue{Float64}
    @test RefStore10363.S[] === 2.5
end

# Non-regression: module-scope unboxed array stores keep converting like
# upstream setindex! (Int value into a Float64 array stores 3.0).
module ArrStore10363
a = zeros(2)
a[1] = 3
b = [1, 2]
b[1] = 7
end

@testset "module-scope array stores keep upstream conversion (Issue #10363)" begin
    @test ArrStore10363.a == [3.0, 0.0]
    @test eltype(ArrStore10363.a) === Float64
    @test ArrStore10363.b == [7, 2]
    @test eltype(ArrStore10363.b) === Int64
end

true
