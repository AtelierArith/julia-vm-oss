# Issue #11146 (Phase 2a of the #10813 exception-parity epic): sjulia's exception
# CLASSES are chosen by one funnel (`VmError::exception_class()`), not ad hoc at
# each raise site. This fixture pins the classes upstream actually raises for the
# constructs the Phase 0 corpus and the Phase 1a fixture-fallout measurement
# found diverging (docs/vm/EXCEPTION_PARITY.md).
#
# Every assertion below was verified against upstream julia 1.12.6 FIRST; this
# file passes under `julia` and `sjulia` alike.
#
# NOTE on style: these use an explicit `try`/`catch` + `@test e isa T` rather than
# `@test_throws T`. `@test_throws` currently ignores its expected type entirely
# (Issue #10354, fixed by the in-flight PR #11163), so a `@test_throws`-based
# type assertion would pass VACUOUSLY on this branch and prove nothing. The
# explicit form is checked today and stays correct after #11163 lands.

using Test
using LinearAlgebra

"Return the exception a thunk throws, or `nothing` if it does not throw."
function thrown_11146(f)
    try
        f()
        return nothing
    catch e
        return e
    end
end

@testset "conversion with no method raises MethodError, not TypeError (Issue #11146)" begin
    # Corpus row `convert_failure`: the SAME TypeError-vs-MethodError class that
    # Issue #10481 closed for `sqrt(::String)`, surviving on an independent call
    # site because each site picked its own "nearest" error.
    s = "a"
    e = thrown_11146(() -> convert(Int, s))
    @test e isa MethodError

    e = thrown_11146(() -> convert(Float64, s))
    @test e isa MethodError

    # A conversion that EXISTS but cannot represent the result stays InexactError
    # (guards against over-correcting the above into a blanket MethodError).
    x = 1.5
    @test thrown_11146(() -> Int(x)) isa InexactError
    @test thrown_11146(() -> convert(Int, x)) isa InexactError
end

@testset "calling a non-callable value raises MethodError (Issue #11146)" begin
    # Corpus row `method_error_noncallable`. NB: a bare numeric literal before
    # `(` is multiplication in Julia (`2(3) == 6`), so the value must be bound to
    # a variable to force a genuine non-callable call.
    z = 5
    @test thrown_11146(() -> z(1)) isa MethodError

    t = (1, 2)
    @test thrown_11146(() -> t(3)) isa MethodError

    str = "abc"
    @test thrown_11146(() -> str(3)) isa MethodError
end

@testset "arity, bounds and shape errors keep their upstream classes (Issue #11146)" begin
    # `<:` with the wrong arity is an upstream ArgumentError (jl_too_few_args);
    # sjulia raised a TypeError whose MESSAGE merely began "ArgumentError: ".
    @test thrown_11146(() -> (<:)(Number)) isa ArgumentError

    # A negative Memory size is an upstream ArgumentError (same mislabel).
    n = -1
    @test thrown_11146(() -> Memory{Int64}(undef, n)) isa ArgumentError

    # Out-of-bounds Memory access is a BoundsError, not a TypeError carrying a
    # "BoundsError: " text prefix.
    m = Memory{Int64}(undef, 3)
    @test thrown_11146(() -> m[10]) isa BoundsError

    # A byte index inside a multi-byte character is a StringIndexError.
    su = "αβγ"
    @test thrown_11146(() -> su[2]) isa StringIndexError

    # A shape mismatch is a DimensionMismatch, not an ErrorException whose text
    # merely says "DimensionMismatch: ..." (the same defect one layer up, in the
    # pure-Julia LinearAlgebra sources).
    D = Diagonal([1.0, 2.0])
    A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]
    @test thrown_11146(() -> D * A) isa DimensionMismatch
end

@testset "a type assertion still raises TypeError (Issue #11146)" begin
    # The funnel must not over-correct: `typeassert` is genuinely a TypeError on
    # both sides, and stays one.
    y = 1
    @test thrown_11146(() -> y::String) isa TypeError
end

@testset "every caught value is an Exception (Issue #11146)" begin
    # The taxonomy's core invariant: whatever sjulia lets you catch is a real
    # Julia exception object, never a bare String. (Before #11146 an `eval`-time
    # feature gap bound a raw `String`, so `typeof(e)` was not even an Exception
    # subtype — see the eval assertions in
    # tests/fixtures/types/signature_forward_reference_11025.jl.)
    # NB: the locals below are deliberately named uniquely (`noncallable_11146`,
    # `mem_11146`) rather than reusing `z`/`m` from the testsets above: a
    # lambda-local assignment whose name collides with an earlier sibling scope's
    # local is currently mis-lowered as a capture (Issue #11190, filed from this
    # fixture; upstream julia runs it fine).
    noncallable_11146 = 5
    mem_11146 = Memory{Int64}(undef, 2)
    for f in (
        () -> convert(Int, "a"),
        () -> noncallable_11146(1),
        () -> mem_11146[9],
        () -> ("αβγ")[2],
        () -> sqrt("a"),
        () -> [1, 2, 3][9],
        () -> Dict(:a => 1)[:missing_key],
    )
        e = thrown_11146(f)
        @test e isa Exception
    end
end

true
