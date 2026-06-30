using Test

abstract type DispatchAnimal3910 end
struct DispatchDog3910 <: DispatchAnimal3910 end
struct DispatchCat3910 <: DispatchAnimal3910 end

typed_dispatch_covariant_resolver_3910(::Type{<:DispatchAnimal3910}) = "animal"
typed_dispatch_covariant_resolver_3910(::Type{DispatchDog3910}) = "dog"

function typed_dispatch_covariant_resolver_via_any_3910(t)
    u::Any = t
    typed_dispatch_covariant_resolver_3910(u)
end

@testset "typed dispatch covariant resolver (Issue #3910)" begin
    @test typed_dispatch_covariant_resolver_via_any_3910(DispatchDog3910) == "dog"
    @test typed_dispatch_covariant_resolver_via_any_3910(DispatchCat3910) == "animal"
end

true
