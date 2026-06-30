using Test

# Issue #6187 / #5936: generated returned Expr(:call, GlobalRef(...), ...)
# should resolve the GlobalRef callee instead of rejecting it as a non-Symbol.

@generated function generated_globalref_add_call_6187(x)
    return Expr(:call, GlobalRef(Base, :+), :x, 4)
end

@generated function generated_globalref_mul_call_6187(x)
    return Expr(:call, GlobalRef(Base, :*), :x, 3)
end

@testset "generated returned Expr(:call, GlobalRef) eval (Issue #6187)" begin
    @test generated_globalref_add_call_6187(6) == 10
    @test generated_globalref_add_call_6187(10) == 14
    @test generated_globalref_mul_call_6187(6) == 18
    @test generated_globalref_mul_call_6187(10) == 30
end

generated_globalref_add_call_6187(6) == 10 &&
    generated_globalref_add_call_6187(10) == 14 &&
    generated_globalref_mul_call_6187(6) == 18 &&
    generated_globalref_mul_call_6187(10) == 30
