# Inner constructor body must run even when the argument count/types match the
# raw field layout (untyped fields => Any). Regression test for Issue #7345:
# the default field-constructor fast path used to fire here, silently skipping
# the inner constructor body (the `*10` transform and the validation `error`).

using Test

struct Bar
    x
    function Bar(x)
        new(x * 10)
    end
end

struct Foo
    x
    Foo(x) = (x > 0 || error("x must be positive, got $x"); new(x))
end

@testset "Inner constructor body runs for untyped fields (Issue #7345)" begin
    # Transforming inner constructor: new(x * 10), not the raw field value.
    b = Bar(5)
    @test b.x == 50

    # Validating inner constructor: positive input is accepted unchanged.
    @test Foo(3).x == 3

    # Validating inner constructor: negative input raises from the body.
    @test_throws ErrorException Foo(-1)
end

true  # Test passed
