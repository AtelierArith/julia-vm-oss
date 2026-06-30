using Test

@testset "getfield(Main, Symbol) resolves function bindings (#4621)" begin
    fname = :reduce
    f = getfield(Main, fname)
    @test f == reduce
    @test typeof(f) == typeof(reduce)

    for name in (:reduce, :foldl, :foldr)
        resolved = getfield(Main, name)
        @test typeof(resolved) <: Function
    end
end

true
