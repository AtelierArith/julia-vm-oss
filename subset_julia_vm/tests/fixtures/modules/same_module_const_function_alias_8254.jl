using Test

module M8254
    const f = isempty
    g(x) = f(x)

    _private_isempty(x) = isempty(x)
    const h = _private_isempty
    k(x) = h(x)
end

@testset "same-module const function alias in later method body (Issue #8254)" begin
    @test M8254.f(Int[]) == true
    @test M8254.g(Int[]) == true
    @test M8254.g([1]) == false

    @test M8254.h(Int[]) == true
    @test M8254.k(Int[]) == true
    @test M8254.k([1]) == false
end

true
