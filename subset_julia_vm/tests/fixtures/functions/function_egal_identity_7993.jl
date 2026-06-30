using Test

# Issue #7993: `===` (object identity / egal) on a generic function must hold for
# the function compared with itself (`f === f`), and identity must be preserved
# when the function is stored in and read back from a struct field. Previously
# generic-function values fell through the egal match arm to `_ => false`, so
# `ff === ff` wrongly returned `false`.

ff(x) = 2x
gg(x) = 3x

struct Box
    f
end

@testset "generic function === identity (#7993)" begin
    # A function is === to itself.
    @test ff === ff

    # Identity survives storage in / load from a struct field.
    b1 = Box(ff)
    b2 = Box(b1.f)
    @test b1.f === ff
    @test b2.f === b1.f
    @test b2.f === ff

    # Distinct functions are NOT ===.
    @test !(ff === gg)
    @test ff !== gg

    # !== is the negation of ===.
    @test !(ff !== ff)

    # A function is not === to a non-function value.
    @test !(ff === 5)

    # Built-in / Base functions are singletons too.
    @test sin === sin
    @test !(sin === cos)
    @test (+) === (+)
    @test ff !== sin
end

true
