# Issue #5171: Value::Generator is now boxed (104B payload -> 8B pointer),
# shrinking the Value enum from 112 to 64 bytes. This regression test
# exercises the local-variable store/load path (frame.locals_generator,
# now Box<GeneratorValue>), eager/lazy/filtered/nested generators, collect,
# sum, isa, and reassignment to ensure boxing preserves observable behavior.

using Test

@testset "Boxed Value::Generator round-trips through locals (Issue #5171)" begin
    # Generator bound to a local, consumed via collect.
    g1 = (x^2 for x in 1:5)
    @test collect(g1) == [1, 4, 9, 16, 25]

    # Generator local consumed via sum.
    g2 = (y + 1 for y in [10, 20, 30])
    @test sum(g2) == 63

    # Filtered generator stored in a local.
    g3 = (z for z in 1:10 if iseven(z))
    @test collect(g3) == [2, 4, 6, 8, 10]

    # Nested generator: outer generator over an inner collected generator.
    inner = (a * 2 for a in 1:3)
    outer = (b + 100 for b in collect(inner))
    @test collect(outer) == [102, 104, 106]

    # A generator local is recognized as a Base.Generator instance.
    g4 = (c for c in 1:3)
    @test isa(g4, Base.Generator)

    # Reassign a generator local to a new generator (store path hit twice).
    gg = (m for m in 1:2)
    @test collect(gg) == [1, 2]
    gg = (n * 10 for n in 1:3)
    @test collect(gg) == [10, 20, 30]
end

true  # Test passed
