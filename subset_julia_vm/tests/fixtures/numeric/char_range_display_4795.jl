# Issue #4795 (final display slice): `'a':'e'` constructed correctly and
# iterated correctly (earlier slices) but its `print`/`show`/`string`
# representation leaked the raw codepoint integers (`97:101`) instead
# of the upstream `StepRange{Char, Int}` form (`'a':1:'e'`).
#
# The `RangeValue` for a Char range already tagged `element_type ==
# RangeElementType::Char` after the first slice of #4795, but the
# `Value::Range(r)` arms in `format_value_slow` and `value_to_string`
# only branched on `r.is_float`; the integer-formatting fallback used
# `{r.start}:{r.stop}` which prints the underlying f64 codepoints.
#
# Fix: gate on `r.element_type == Char` before the float/integer
# branches in both arms, converting `start`/`stop` back to `Char`
# via `char::from_u32` and emitting `'start':step:'stop'`. Mirrors
# upstream Julia's StepRange{Char, Int} show form (which always
# shows the explicit step, including for step=1).

using Test

@testset "Char range println (Issue #4795)" begin
    buf = IOBuffer()
    println(buf, 'a':'e')
    @test String(take!(buf)) == "'a':1:'e'\n"
end

@testset "Char range string() (Issue #4795)" begin
    @test string('a':'e') == "'a':1:'e'"
end

@testset "Char range with reverse step (Issue #4795)" begin
    @test string('e':-1:'a') == "'e':-1:'a'"
end

@testset "Char range non-trivial step (Issue #4795)" begin
    @test string('a':2:'g') == "'a':2:'g'"
end

@testset "Integer range regression — no codepoint format leak (Issue #4795)" begin
    # Make sure adding the Char arm did not regress numeric range
    # printing.
    @test string(1:5) == "1:5"
    @test string(1:2:9) == "1:2:9"
end

true
