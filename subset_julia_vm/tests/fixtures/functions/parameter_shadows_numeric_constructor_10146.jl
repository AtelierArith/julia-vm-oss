using Test

call_shadowed_float64(Float64) = Float64(2)
call_shadowed_int64(Int64) = Int64(2)

@testset "parameter shadows numeric constructor (Issue #10146)" begin
    @test call_shadowed_float64(x -> x + 10) == 12
    @test call_shadowed_int64(x -> x * 7) == 14

    # Unshadowed numeric constructors still resolve through the builtin path.
    @test Float64(2) == 2.0
    @test Int64(2.0) == 2
end

# Shadowing matrix over representative name-keyed runtime-specializer
# compile_call fast paths (Issue #10418): every name the specializer matches
# directly ("Float64", "Int64", "sqrt", "round", ...) must resolve a local
# binding of the same name as a callable value, exactly like upstream Julia.
call_shadowed_sqrt(sqrt) = sqrt(4)
call_shadowed_round(round) = round(2.5)

function local_binding_shadows_sqrt()
    sqrt = x -> x - 1
    sqrt(10)
end

@testset "parameter shadows specializer math fast paths (Issue #10418)" begin
    @test call_shadowed_sqrt(x -> x + 3) == 7
    @test call_shadowed_round(x -> x + 0.5) == 3.0

    # A plain local binding (not just a parameter) shadows the builtin too.
    @test local_binding_shadows_sqrt() == 9

    # Unshadowed math builtins still resolve through the builtin path.
    @test sqrt(4.0) == 2.0
    @test round(2.6) == 3.0
end

true
