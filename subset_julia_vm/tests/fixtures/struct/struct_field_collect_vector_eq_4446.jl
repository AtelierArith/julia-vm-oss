# Vector from collect stored in an untyped struct field compares with == (Issue #4446)

using Test

struct Foo4446
    x
end

@testset "collect Vector through struct field keeps array equality (Issue #4446)" begin
    xs = collect(1:10)
    f = Foo4446(xs)

    @test typeof(f.x) == Vector{Int64}
    @test f.x === xs
    @test f.x == xs
    @test !(f.x != xs)
end

true
