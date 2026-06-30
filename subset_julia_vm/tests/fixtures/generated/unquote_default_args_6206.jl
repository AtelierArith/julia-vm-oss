using Test

# Issue #6206 / #5074: once a generated body is syntactically unquoted, it is
# ordinary runtime code. Optional-argument wrappers must not call it with
# positional arguments rebound to generated-time type objects.

@generated function generated_unquote_default_args_6206(x, a=5)
    :(x + a)
end

@generated function generated_unquote_default_return_6206(x, a=5)
    return :(x + a)
end

@generated generated_unquote_default_short_6206(x, a=5) = :(x + a)

@testset "generated unquote default args (Issue #6206)" begin
    @test generated_unquote_default_args_6206(7) == 12
    @test generated_unquote_default_args_6206(7, 6) == 13
    @test generated_unquote_default_return_6206(7) == 12
    @test generated_unquote_default_return_6206(7, 6) == 13
    @test generated_unquote_default_short_6206(7) == 12
    @test generated_unquote_default_short_6206(7, 6) == 13
end

generated_unquote_default_args_6206(7) == 12 &&
    generated_unquote_default_args_6206(7, 6) == 13 &&
    generated_unquote_default_return_6206(7) == 12 &&
    generated_unquote_default_return_6206(7, 6) == 13 &&
    generated_unquote_default_short_6206(7) == 12 &&
    generated_unquote_default_short_6206(7, 6) == 13
