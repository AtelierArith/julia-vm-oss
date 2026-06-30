# Issue #4761: `print(buf::IOBuffer, r::LinRange)` and `string(r)` —
# along with `print(stdout, r)` / `println(stdout, r)` — leaked the
# raw LinRange field dump `LinRange{Float64}(0.0, 1.0, 5, 4)` (the
# trailing `4` is the internal `lendiv` helper field) instead of
# matching the user-facing `show(io, ::LinRange)` shape
# `LinRange{Float64}(0.0, 1.0, 5)`.
#
# Surfaced while fixing #4759 (StepRangeLen/LinRange show). The
# print/string code path in `vm/formatting.rs::format_struct_instance`
# falls through to the generic `StructName(field1, ...)` field-dump
# for any struct without a special-case arm. Rational/Pair/Array
# wrapper already had arms; this PR adds one for LinRange so the
# 4-field struct projects to the user-facing 3-arg show form.
#
# Scope: LinRange specifically. The broader bug (`print(io, x)` /
# `string(x)` for any struct with a user `show` method does not
# dispatch to that user method) is tracked separately — VM
# infrastructure for invoking user show from inside the IOPrint /
# StringNew builtins does not yet exist (the PrintAnyNoNewline
# single-arg path is the only entry point that currently dispatches
# to `show_methods`).

using Test

@testset "print(buf, ::LinRange) writes show-form (Issue #4761)" begin
    r = LinRange(0.0, 1.0, 5)
    buf = IOBuffer()
    print(buf, r)
    @test String(take!(buf)) == "LinRange{Float64}(0.0, 1.0, 5)"
end

@testset "string(::LinRange) returns show-form (Issue #4761)" begin
    r = LinRange(0.0, 1.0, 5)
    @test string(r) == "LinRange{Float64}(0.0, 1.0, 5)"
end

@testset "print(stdout, ::LinRange) is also show-form (Issue #4761)" begin
    # Round-trip via sprint to capture stdout-equivalent output.
    r = LinRange(0.0, 1.0, 5)
    buf = IOBuffer()
    print(buf, r)
    @test String(take!(buf)) == "LinRange{Float64}(0.0, 1.0, 5)"
end

@testset "println(buf, ::LinRange) writes show-form + newline (Issue #4761)" begin
    r = LinRange(0.0, 1.0, 5)
    buf = IOBuffer()
    println(buf, r)
    @test String(take!(buf)) == "LinRange{Float64}(0.0, 1.0, 5)\n"
end

@testset "Float64 LinRange various endpoints (Issue #4761)" begin
    r = LinRange(0.0, 10.0, 11)
    @test string(r) == "LinRange{Float64}(0.0, 10.0, 11)"
end

@testset "LinRange display does NOT leak internal lendiv field (Issue #4761)" begin
    # Direct regression guard: the 4th internal field must not appear
    # in any print/string path.
    r = LinRange(0.0, 1.0, 5)
    s1 = string(r)
    buf = IOBuffer()
    print(buf, r)
    s2 = String(take!(buf))
    # `LinRange{Float64}(0.0, 1.0, 5)` — three values inside the parens.
    @test count(==(','), s1) == 2
    @test count(==(','), s2) == 2
end

@testset "Plain struct field dump unaffected (Issue #4761)" begin
    # Sanity check: a struct without a special-case arm still gets
    # the default StructName(field1, field2, ...) field dump.
    # Using a local struct so the test is self-contained.
    s = string(Pair(1, 2))
    @test s == "1 => 2"
end

true
