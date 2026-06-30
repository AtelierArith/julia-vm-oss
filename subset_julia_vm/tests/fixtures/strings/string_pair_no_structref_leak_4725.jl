using Test

@testset "string(Pair) renders 'a => b' instead of leaking StructRef (Issue #4725)" begin
    p = Pair(1, 2)
    @test string(p) == "1 => 2"
    @test repr(p) == "1 => 2"

    # Symbol field follows show semantics inside Pair: `:` prefix kept.
    @test string(Pair(:x, 3.14)) == ":x => 3.14"
    # NOTE: String interpolation (`"$p"`) still leaks the StructRef
    # debug repr — the interpolation lowering uses a different path than
    # the string() builtin. Tracked separately.
    # NOTE: String fields inside Pair print without quotes
    # (`string(Pair("a", 42))` returns `"a => 42"` in sjulia, but
    # upstream Julia uses show semantics there → `"\"a\" => 42"`).
    # Tracked as a separate show-vs-print parity gap.
end

@testset "string(Pair) survives nesting inside Tuple (Issue #4725)" begin
    p = Pair(1, 2)
    @test string((1, p)) == "(1, 1 => 2)"
    @test string((p, p)) == "(1 => 2, 1 => 2)"
    # Ref display intentionally diverges between sjulia ("Ref(1 => 2)")
    # and upstream ("Base.RefValue{...}(1 => 2)"); not part of #4725.
end

true
