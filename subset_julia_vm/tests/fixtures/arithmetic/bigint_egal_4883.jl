# Issue #4883: `x === x` returned `false` for a `BigInt` value bound to
# the same local — breaking reflexivity. Surfaced while writing the
# carrier-matrix testset for #4878 (`ndims(::Char)`); the assertion
# `x[1] === x` failed only for `x = big(7)`.
#
# Root cause: the `Egal` builtin in
# `subset_julia_vm/src/vm/builtins_equality.rs` had no
# `(Value::BigInt, Value::BigInt)` arm, so comparison fell through to
# `_ => false`. Same gap for `BigFloat`.
#
# Initial fix: add `===` arms for both `BigInt` and `BigFloat` so
# same-binding and alias identity are reflexive. Issue #4886 later moved
# sjulia to upstream-compatible reference identity for independently
# allocated same-value BigInt/BigFloat objects.

using Test

@testset "BigInt === reflexivity for same binding (Issue #4883)" begin
    # Reflexivity is required by every value comparator. Same
    # variable on both sides — upstream Julia and sjulia agree.
    x = big(7)
    @test x === x
    @test x !== big(8)   # clearly different value

    y = big(0)
    @test y === y

    z = big(-5)
    @test z === z
end

@testset "BigInt alias === preserves identity (Issue #4883)" begin
    # `y = x` shares the same BigInt under both interpretations;
    # `y === x` is true upstream and now true in sjulia too.
    x = big(7)
    y = x
    @test y === x

    a = big(100)
    b = a
    @test b === a
end

@testset "BigFloat === reflexivity for same binding (Issue #4883)" begin
    x = big(7.0)
    @test x === x

    y = big(3.14)
    @test y === y

    # Alias case.
    z = y
    @test z === y
end

@testset "Number primitive === arms remain reflexive (regression guard)" begin
    # The primitive numeric arms already worked before #4883; pin
    # them so the BigInt/BigFloat addition doesn't shadow or
    # regress them.
    @test 7 === 7
    @test 3.14 === 3.14
    @test true === true
    @test Int32(5) === Int32(5)
    @test UInt8(255) === UInt8(255)
    @test !(7 === 8)
    @test !(7 === 7.0)   # different concrete types
end

true
