# Issue #6736: convert / promote public API dispatch through pure-Julia
# methods (base/essentials.jl, base/promotion.jl). The compile-time routing is
# dispatch-first, so user-defined `convert` / `promote_rule` methods are NOT
# shadowed by the Rust fallback (which only performs the primitive numeric
# conversion for types without a pure method). Verified against julia 1.12.

using Test

@testset "built-in numeric convert / promote (Issue #6736)" begin
    @test convert(Float64, 3) === 3.0
    @test convert(Int, 3.0) === 3
    @test convert(Int8, 5) === Int8(5)
    @test convert(Float32, 1) === 1.0f0
    @test promote(1, 2.0) === (1.0, 2.0)
    @test promote(1, 2, 3.0) === (1.0, 2.0, 3.0)
    @test promote(Int8(1), 2.0) === (1.0, 2.0)
    @test promote_type(Int, Float64) === Float64
    @test promote_type(Int8, Int16) === Int16
end

struct MyWrap
    x::Int
end
import Base: convert
convert(::Type{MyWrap}, n::Int) = MyWrap(n)

@testset "user-defined convert is dispatch-first, not shadowed (Issue #6736)" begin
    @test convert(MyWrap, 5).x == 5
    @test convert(MyWrap, 42) isa MyWrap
    # explicit convert in a comprehension keeps dispatching to the user method
    w = [convert(MyWrap, i) for i in 1:3]
    @test length(w) == 3
    @test w[2].x == 2
end

struct Money
    cents::Int
end
import Base: promote_rule
promote_rule(::Type{Money}, ::Type{Int}) = Money
convert(::Type{Money}, n::Int) = Money(n)
convert(::Type{Money}, m::Money) = m

@testset "user promote_rule + convert participate in promote (Issue #6736)" begin
    p = promote(Money(100), 5)
    @test p isa Tuple{Money,Money}
    @test p[1].cents == 100
    @test p[2].cents == 5
    @test promote_type(Money, Int) === Money
end

true
