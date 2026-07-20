# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: strings/html_literal.jl =====
# Test html"..." string literal (Issue #468)
# Tests that html"text" creates an HTML{String} object


@testset "HTML string literal" begin
    # Basic html literal
    h = html"<b>bold</b>"
    @test isa(h, HTML{String})
    @test h.content == "<b>bold</b>"

    # HTML with various content
    h2 = html"<div>hello world</div>"
    @test isa(h2, HTML{String})
    @test h2.content == "<div>hello world</div>"

    # Empty HTML
    h3 = html""
    @test isa(h3, HTML{String})
    @test h3.content == ""

    # HTML equality
    @test html"test" == html"test"
    @test !(html"a" == html"b")
end

# ===== source: strings/string_base_int_widths_4723.jl =====

@testset "string(x; base=N) accepts all signed integer widths (Issue #4723)" begin
    @test string(Int8(-42), base=10) == "-42"
    @test string(Int8(-1), base=16) == "-1"
    @test string(Int16(255), base=16) == "ff"
    @test string(Int32(-1024), base=10) == "-1024"
    @test string(Int64(255), base=16) == "ff"
    @test string(Int128(1) << 100, base=16) == "10000000000000000000000000"
    @test string(Int128(-1), base=10) == "-1"
end

@testset "string(x; base=N) accepts all unsigned integer widths (Issue #4723)" begin
    @test string(UInt8(255), base=16) == "ff"
    @test string(UInt8(255), base=2) == "11111111"
    @test string(UInt16(1000), base=2) == "1111101000"
    @test string(UInt32(0xCAFEBABE), base=16) == "cafebabe"
    @test string(UInt64(1) << 60, base=16) == "1000000000000000"
    @test string(typemax(UInt128), base=16) == "ffffffffffffffffffffffffffffffff"
end

@testset "string(x; base=N) accepts Bool (Issue #4723)" begin
    @test string(true, base=10) == "1"
    @test string(true, base=2) == "1"
    @test string(false, base=10) == "0"
    @test string(false, base=16) == "0"
end

@testset "string(x; base=N) bases 2/8/10/16 and generic 2..36 (Issue #4723)" begin
    @test string(255, base=8) == "377"
    @test string(255, base=36) == "73"
    @test string(UInt8(255), base=36) == "73"
    @test string(Int32(-255), base=36) == "-73"
end

# ===== source: strings/string_bitstring.jl =====
# Test bitstring function - binary representation as string


@testset "bitstring(x) - binary representation as string" begin

    # === Int64 ===
    # Positive integers
    bs5 = bitstring(5)
    @assert length(bs5) == 64
    @assert endswith(bs5, "0101")
    @assert startswith(bs5, "0000000000000000000000000000000000000000000000000000000000000")

    # Zero
    bs0 = bitstring(0)
    @assert length(bs0) == 64
    @assert bs0 == "0000000000000000000000000000000000000000000000000000000000000000"

    # Negative integers (two's complement)
    bsn1 = bitstring(-1)
    @assert length(bsn1) == 64
    @assert bsn1 == "1111111111111111111111111111111111111111111111111111111111111111"

    # === Float64 ===
    # Positive float
    bs1_5 = bitstring(1.5)
    @assert length(bs1_5) == 64
    # IEEE 754 representation of 1.5

    # Zero float
    bs0_0 = bitstring(0.0)
    @assert length(bs0_0) == 64
    @assert bs0_0 == "0000000000000000000000000000000000000000000000000000000000000000"

    # === Bool ===
    # Bool is 1 byte → 8 bits (matches upstream; the old Rust builtin wrongly
    # returned "1"/"0", fixed when bitstring became pure Julia — Issue #6747).
    bstrue = bitstring(true)
    @assert bstrue == "00000001"

    bsfalse = bitstring(false)
    @assert bsfalse == "00000000"

    # All tests passed
    @test (true)
end

# ===== source: strings/string_hex2bytes.jl =====
# Test hex2bytes function - convert hex string to byte array


@testset "hex2bytes(s) - convert hex string to byte array" begin

    # === Basic conversion ===

    # Simple hex string
    bytes1 = hex2bytes("48656c6c6f")
    @assert length(bytes1) == 5
    @assert bytes1[1] == 72   # 0x48
    @assert bytes1[2] == 101  # 0x65
    @assert bytes1[3] == 108  # 0x6c
    @assert bytes1[4] == 108  # 0x6c
    @assert bytes1[5] == 111  # 0x6f

    # Empty string
    bytes2 = hex2bytes("")
    @assert length(bytes2) == 0

    # Single byte
    bytes3 = hex2bytes("ff")
    @assert length(bytes3) == 1
    @assert bytes3[1] == 255

    # Zero byte
    bytes4 = hex2bytes("00")
    @assert length(bytes4) == 1
    @assert bytes4[1] == 0

    # === Various hex values ===

    # Lowercase hex
    bytes5 = hex2bytes("deadbeef")
    @assert length(bytes5) == 4
    @assert bytes5[1] == 222  # 0xde
    @assert bytes5[2] == 173  # 0xad
    @assert bytes5[3] == 190  # 0xbe
    @assert bytes5[4] == 239  # 0xef

    # Uppercase hex (should also work)
    bytes6 = hex2bytes("DEADBEEF")
    @assert length(bytes6) == 4
    @assert bytes6[1] == 222  # 0xde
    @assert bytes6[2] == 173  # 0xad
    @assert bytes6[3] == 190  # 0xbe
    @assert bytes6[4] == 239  # 0xef

    # Mixed case
    bytes7 = hex2bytes("DeAdBeEf")
    @assert length(bytes7) == 4

    # Leading zeros
    bytes8 = hex2bytes("01020a0f")
    @assert bytes8[1] == 1
    @assert bytes8[2] == 2
    @assert bytes8[3] == 10
    @assert bytes8[4] == 15

    # All tests passed
    @test (true)
end

# ===== source: strings/string_interpolation_pair_4727.jl =====

@testset "string interpolation of Pair does not leak StructRef (Issue #4727)" begin
    p = Pair(1, 2)
    @test "$p" == "1 => 2"
    @test "Wrapped: $p" == "Wrapped: 1 => 2"
    @test "$p, $p" == "1 => 2, 1 => 2"

    # Symbol field follows show semantics inside Pair
    @test "$(Pair(:x, 3.14))" == ":x => 3.14"
end

@testset "string interpolation resolves nested Pair inside Tuple (Issue #4727)" begin
    p = Pair(1, 2)
    @test "$((1, p))" == "(1, 1 => 2)"
    @test "$((p, p))" == "(1 => 2, 1 => 2)"
end

# ===== source: strings/string_pair_leak_guard_matrix_4729.jl =====

# Matrix-style leak guard for Issue #4729 / Issues #4725 #4727: every
# value-to-string entry point in sjulia must render a heap-allocated
# `Pair` as "1 => 2", never as the Rust debug repr
# "StructRef(heap_idx=N)". A regression in any one of these paths is
# user-visible (silent wrong output).

@testset "Pair value-to-string leak guard matrix (Issue #4729)" begin
    p = Pair(1, 2)

    # string() builtin — covered by PR #4726 (Issue #4725)
    @test string(p) == "1 => 2"

    # repr() builtin — covered by PR #4726 (Issue #4725)
    @test repr(p) == "1 => 2"

    # String interpolation — covered by PR #4728 (Issue #4727)
    @test "$p" == "1 => 2"
    @test "Wrapped: $p" == "Wrapped: 1 => 2"

    # NOTE: sjulia's `sprintf` (covered in this PR) is exposed as a
    # Base-level function; upstream Julia uses `Printf.@sprintf`, so
    # the sprintf parity assertions live in a separate sjulia-only
    # fixture and are not in this upstream-parity matrix.

    # string() composition with other args
    @test string("Wrapped: ", p) == "Wrapped: 1 => 2"
    @test string(p, " end") == "1 => 2 end"

    # Tuple/Ref/QuoteNode carriers preserved across all entry points
    @test string((1, p)) == "(1, 1 => 2)"
    @test "$((1, p))" == "(1, 1 => 2)"
end

# ===== source: strings/string_pair_no_structref_leak_4725.jl =====

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

# ===== source: strings/string_parse_base.jl =====
# string(x; base=N) and parse(T, s; base=N) - number base conversion (Issue #2036)


@testset "string(x; base=N)" begin
    # Hexadecimal (base 16)
    @test string(255, base=16) == "ff"
    @test string(0, base=16) == "0"
    @test string(16, base=16) == "10"

    # Binary (base 2)
    @test string(255, base=2) == "11111111"
    @test string(0, base=2) == "0"
    @test string(10, base=2) == "1010"

    # Octal (base 8)
    @test string(255, base=8) == "377"
    @test string(8, base=8) == "10"
    @test string(10, base=8) == "12"

    # Decimal (base 10) - identity
    @test string(42, base=10) == "42"
end

@testset "parse(Int, s; base=N)" begin
    # Hexadecimal (base 16)
    @test parse(Int, "ff", base=16) == 255
    @test parse(Int, "10", base=16) == 16
    @test parse(Int, "0", base=16) == 0

    # Binary (base 2)
    @test parse(Int, "11111111", base=2) == 255
    @test parse(Int, "1010", base=2) == 10

    # Octal (base 8)
    @test parse(Int, "377", base=8) == 255
    @test parse(Int, "12", base=8) == 10

    # Decimal (base 10)
    @test parse(Int, "42", base=10) == 42
end

@testset "round-trip: parse(Int, string(x; base=N); base=N) == x" begin
    @test parse(Int, string(100, base=16), base=16) == 100
    @test parse(Int, string(42, base=2), base=2) == 42
    @test parse(Int, string(73, base=8), base=8) == 73
end

# Issue #7875 (docs/COMPARISION.md P1): parse(Int, s; base=N) is now Pure Julia
# (`_parse_int_base` wrapping `_tryparse_int` in base/parse.jl); the compiler
# rewrites the kwargs form to a positional call instead of the former
# `StringToIntBase` Rust builtin. These cover the edge cases the migration must
# preserve (sign, surrounding whitespace, max base-36 digit, mixed-case hex).
@testset "parse(Int, s; base=N) pure-Julia migration (#7875)" begin
    @test parse(Int, "  -101  ", base=2) == -5
    @test parse(Int, "+ff", base=16) == 255
    @test parse(Int, "z", base=36) == 35
    @test parse(Int, "DEAD", base=16) == 57005
    @test parse(Int, "dead", base=16) == 57005
end

# Issue #7942: `_` is a digit separator only in numeric *literals* in source
# code, not in parse()/tryparse() string input. Upstream julia throws
# ArgumentError (parse) / returns nothing (tryparse). The pre-existing
# underscore-skip in `_tryparse_int` (introduced by #2566) was removed, fixing
# the base-10 path and keeping the migrated base-N path upstream-faithful.
@testset "parse/tryparse reject underscores (#7942)" begin
    @test tryparse(Int, "1_000") === nothing
    @test_throws ArgumentError parse(Int, "1_000")
    @test_throws ArgumentError parse(Int, "ff_ff", base=16)
    # plain digit strings still parse correctly
    @test parse(Int, "1000") == 1000
    @test parse(Int, "ffff", base=16) == 65535
end

# ===== source: strings/string_tryparse.jl =====
# Test tryparse function - parse string with nothing on failure


@testset "tryparse(T, s) - parse string, return nothing on failure" begin

    # === Parse Int64 ===
    @assert tryparse(Int64, "123") == 123
    @assert tryparse(Int64, "-456") == -456
    @assert tryparse(Int64, "0") == 0
    @assert tryparse(Int64, "  789  ") == 789  # trimmed

    # === Parse Int64 failures ===
    @assert tryparse(Int64, "abc") === nothing
    @assert tryparse(Int64, "12.34") === nothing
    @assert tryparse(Int64, "") === nothing
    @assert tryparse(Int64, "   ") === nothing

    # === Parse Float64 ===
    @assert tryparse(Float64, "3.14") == 3.14
    @assert tryparse(Float64, "-2.5") == -2.5
    @assert tryparse(Float64, "0.0") == 0.0
    @assert tryparse(Float64, "  1.5  ") == 1.5  # trimmed
    @assert tryparse(Float64, "42") == 42.0  # int to float

    # === Parse Float64 failures ===
    @assert tryparse(Float64, "abc") === nothing
    @assert tryparse(Float64, "") === nothing

    # === Using Int alias ===
    @assert tryparse(Int, "100") == 100
    @assert tryparse(Int, "xyz") === nothing

    # All tests passed
    @test (true)
end

# ===== source: strings/text_literal.jl =====
# Test text"..." string literal (Issue #468)
# Tests that text"string" creates a Text{String} object


@testset "Text string literal" begin
    # Basic text literal
    t = text"hello world"
    @test isa(t, Text{String})
    @test t.content == "hello world"

    # Text with special characters
    t2 = text"line1\nline2"
    @test isa(t2, Text{String})

    # Empty text
    t3 = text""
    @test isa(t3, Text{String})
    @test t3.content == ""

    # Text equality
    @test text"test" == text"test"
    @test !(text"a" == text"b")
end

true
