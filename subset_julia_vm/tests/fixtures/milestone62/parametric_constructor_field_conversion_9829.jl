using Test

struct Foo9829{T}
    value::T
end

struct Bar9829{T}
    value::T
end

function runtime_foo9829(t)
    Foo9829{t}(1 + 1im)
end

function runtime_bar9829(t)
    Bar9829{t}(1)
end

@testset "runtime parametric constructors convert fields" begin
    foo = runtime_foo9829(ComplexF64)
    @test typeof(foo) == Foo9829{ComplexF64}
    @test typeof(foo.value) == ComplexF64
    @test real(foo.value) == 1.0
    @test imag(foo.value) == 1.0

    bar = runtime_bar9829(Float64)
    @test typeof(bar) == Bar9829{Float64}
    @test typeof(bar.value) == Float64
    @test bar.value == 1.0
end

true
