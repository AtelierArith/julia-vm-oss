# `Vector{e}` (a literal, compile-time-known base with an expression-position
# type parameter — `Instr::ConstructParametricType`) must validate the
# parameter value exactly like `Core.apply_type`'s dynamic-base path does: an
# `ErrorException` instance or a `String` is neither a `Type`, a `TypeVar`, a
# `Symbol`, nor an `isbits` value, so upstream raises `TypeError` instead of
# constructing anything. sjulia used to call `build_parametric_type` directly
# with no such check, silently degrading the unrecognized parameter to the
# `Any` placeholder (`Vector{Any}`) instead of raising (Issue #11555).
#
# The valid isbits case (`Vector{7}`, Issue #4644) must keep working.

using Test

@testset "ConstructParametricType invalid parameter raises TypeError (Issue #11555)" begin
    e = ErrorException("z")
    err = try
        Vector{e}
        nothing
    catch caught
        caught
    end
    @test typeof(err) == TypeError

    @test_throws TypeError Vector{e}
    @test_throws TypeError Vector{"not a type"}

    s = "also not a type"
    @test_throws TypeError Vector{s}

    # Valid isbits value parameter still works (Issue #4644 regression guard).
    x = 7
    @test Vector{x} === Vector{7}
    @test string(Vector{x}) == "Vector{7}"
end

@testset "ConstructParametricType keeps accepting other upstream-valid isbits parameters (Issue #11555)" begin
    # `Complex`/`Rational` are immutable, all-isbits-field structs upstream —
    # legal type parameters, not `ErrorException`-style invalid values. sjulia
    # renders them (and any struct-value parameter) as the `Any` placeholder
    # regardless — a separate, pre-existing display-only limitation this fix
    # does not change — so only assert they do NOT raise.
    z = 1 + 2im
    r = 1 // 2
    @test (try
        Vector{z}
        true
    catch
        false
    end)
    @test (try
        Vector{r}
        true
    catch
        false
    end)

    # A `NamedTuple` with isbits fields is isbits upstream too, like `Tuple`.
    nt = (a = 1, b = 2)
    @test (try
        Vector{nt}
        true
    catch
        false
    end)

    # `nothing`/`missing`/a `Module` are isbits/valid parameters upstream too.
    @test (try
        Vector{nothing}
        true
    catch
        false
    end)
    @test (try
        Vector{missing}
        true
    catch
        false
    end)
    @test (try
        Vector{Base}
        true
    catch
        false
    end)

    # A bare named `Function` (no captures) and an `@enum` value are isbits
    # upstream — must NOT newly raise TypeError (this is the exact class of
    # false positive this fix must avoid: turning an upstream-valid value
    # into a new error instead of leaving the pre-existing Any rendering).
    @test (try
        Vector{sin}
        true
    catch
        false
    end)

    @enum ColorConstructParametric11555 RedCP11555 GreenCP11555 BlueCP11555
    @test (try
        Vector{RedCP11555}
        true
    catch
        false
    end)

    comp = sin ∘ cos
    @test (try
        Vector{comp}
        true
    catch
        false
    end)

    # A non-isbits `Number` (`BigInt`/`BigFloat` are mutable upstream) still
    # raises TypeError, with `expected Int64` rather than `expected Type`.
    big_i = big(5)
    big_f = big(5.0)
    err_big_i = try
        Vector{big_i}
        nothing
    catch caught
        caught
    end
    @test typeof(err_big_i) == TypeError
    @test_throws TypeError Vector{big_i}
    @test_throws TypeError Vector{big_f}
end

true
