# Issue #10194: a `struct` definition nested inside a `@testset "..." begin
# ... end` block must lower and run, matching upstream Julia. `Test.@testset`
# expands its body into a `let ... end` (Issue #9312's hard-scope expansion),
# and upstream Julia allows a nested `struct` inside `let`/`begin` wherever
# that block is itself reachable from top level — the block does not count
# as a function boundary. sjulia previously failed to lower this with
# `UnsupportedFeature { kind: UnsupportedExpression("struct_definition") }`.

using Test

@testset "nested struct def" begin
    struct Foo10091
        x::Int
    end
    @test Foo10091(1).x == 1
end

# A mutable struct nested the same way must also work.
@testset "nested mutable struct def" begin
    mutable struct MutableFoo10194
        y::Int
    end
    m = MutableFoo10194(5)
    m.y = 10
    @test m.y == 10
end

true
