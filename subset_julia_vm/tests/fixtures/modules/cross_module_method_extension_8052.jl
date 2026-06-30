using Test

module Inner8052
    f8052(x::Int) = "Inner.f(::Int)"
    export f8052
end
using .Inner8052

# Full-form extension of another module's function (`function Mod.f`).
function Inner8052.f8052(x::Float64)
    "outer Inner.f(::Float64)"
end

# Short-form extension of another module's function (`Mod.f(...) = ...`).
Inner8052.f8052(x::String) = "outer Inner.f(::String)"

# Variant: `import Mod: f` then a bare `function f(...)` must EXTEND Mod.f
# (join its method table), not shadow it.
module Inner8052B
    g8052(x::Int) = "InnerB.g(::Int)"
    export g8052
end
import .Inner8052B: g8052
function g8052(x::Float64)
    "outer g(::Float64)"
end

@testset "extend another module's function (Issue #8052)" begin
    # qualified-definition form: both unqualified and qualified calls dispatch
    @test f8052(1) == "Inner.f(::Int)"
    @test f8052(2.0) == "outer Inner.f(::Float64)"
    @test f8052("hi") == "outer Inner.f(::String)"
    @test Inner8052.f8052(1) == "Inner.f(::Int)"
    @test Inner8052.f8052(2.0) == "outer Inner.f(::Float64)"
    @test Inner8052.f8052("hi") == "outer Inner.f(::String)"

    # import-variant: a bare definition extends the imported function
    @test g8052(1) == "InnerB.g(::Int)"
    @test g8052(2.0) == "outer g(::Float64)"
    @test Inner8052B.g8052(1) == "InnerB.g(::Int)"
    @test Inner8052B.g8052(2.0) == "outer g(::Float64)"
end

true
