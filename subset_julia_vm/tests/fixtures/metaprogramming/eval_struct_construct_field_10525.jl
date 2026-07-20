# Runtime eval() struct construction and field reads (Issue #10525)
#
# The eval mini-interpreter can call an already-compiled struct's default
# constructor by name (`eval(:(Foo(1)))` routes the bare struct name as a
# type-object callable) and read fields via dot syntax (`eval(:(f.x))`
# desugars to `getfield(obj, :name)`, Julia's own lowering shape).

using Test

mutable struct EvalFoo10525
    x::Int
end

struct EvalBar10525
    a::Int
    b::String
end

@testset "eval constructs compiled structs by name" begin
    r1 = eval(:(EvalFoo10525(1)))
    @test r1 isa EvalFoo10525
    @test r1.x == 1
    r3 = eval(:(EvalBar10525(2, "hi")))
    @test r3.a == 2
    @test r3.b == "hi"
end

# Upstream eval evaluates in global (module) scope, so the values the
# evaluated expressions reference must be globals.
f10525 = EvalFoo10525(41)
bar10525 = EvalBar10525(7, "s")

@testset "eval reads struct fields via dot syntax" begin
    @test eval(:(f10525.x)) == 41
    @test eval(:(bar10525.a)) == 7
    @test eval(:(bar10525.b)) == "s"
    # nested expression using a field read
    @test eval(:(f10525.x + 1)) == 42
    # getfield spelled explicitly keeps working
    @test eval(:(getfield(bar10525, :a))) == 7
end

true
