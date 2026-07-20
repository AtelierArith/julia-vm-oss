# Runtime string-index failures must carry the exact offending String through
# the shared exception funnel. The index-vector case also pins the class fix
# discovered while auditing the producers (Issues #11572/#11615). Range
# endpoints also use Julia code-unit indices rather than Rust exclusive byte
# ends (Issue #11618). Byte-backed strings must use that same route rather than
# falling through to generic getindex dispatch (Issue #11627). True numeric
# out-of-bounds remains BoundsError, whose receiver payload is tracked by
# #11616.
using Test

function caught_11572(f)
    try
        f()
        return nothing
    catch e
        return e
    end
end

s11572 = "é"

@testset "StringIndexError carries its receiver (Issues #11572/#11615/#11618/#11627)" begin
    scalar = caught_11572(() -> s11572[2])
    @test scalar isa StringIndexError
    @test scalar.string === s11572
    @test scalar.index == 2

    ranged = caught_11572(() -> s11572[1:2])
    @test ranged isa StringIndexError
    @test ranged.string === s11572
    @test ranged.index == 2

    # The final caller index names a character start; only after validating it
    # may the VM advance to Rust's exclusive byte slice end (Issue #11618).
    multichar = "éx"
    @test multichar[1:1] == "é"
    @test multichar[1:3] == "éx"
    @test s11572[:] == s11572

    stepped = 1:2:3
    explicit_unit_step = 1:1:3
    descending = 3:-1:1
    @test "abc"[stepped] == "ac"
    @test "abc"[explicit_unit_step] == "abc"
    @test "abc"[descending] == "cba"

    # A StepRange remains an elementwise index even when its numeric step is
    # one. It must therefore validate every requested String code-unit start,
    # unlike the contiguous UnitRange slice route (Issue #11629).
    explicit_unit_error = caught_11572(() -> "éx"[explicit_unit_step])
    @test explicit_unit_error isa StringIndexError
    @test explicit_unit_error.string == "éx"
    @test explicit_unit_error.index == 2

    indexed = caught_11572(() -> s11572[[2]])
    @test indexed isa StringIndexError
    @test indexed.string === s11572
    @test indexed.index == 2

    # The invalid code-unit boundary above is a StringIndexError. An index
    # beyond the string's code-unit bounds remains BoundsError; restoring that
    # BoundsError's `.a` field is deliberately separate (Issue #11616).
    @test caught_11572(() -> s11572[[3]]) isa BoundsError

    # Invalid UTF-8 is a supported String representation. A valid malformed
    # character start preserves its exact bytes; a continuation-byte endpoint
    # raises StringIndexError with that exact byte-backed receiver (#11627).
    malformed = String(UInt8[0xf0, 0x80, 0x80, 0x80])
    @test collect(codeunits(malformed[:])) == UInt8[0xf0, 0x80, 0x80, 0x80]
    @test collect(codeunits(malformed[1:1])) == UInt8[0xf0, 0x80, 0x80, 0x80]
    @test collect(codeunits(malformed[[1, 1]])) == UInt8[0xf0, 0x80, 0x80, 0x80, 0xf0, 0x80, 0x80, 0x80]

    malformed_range = caught_11572(() -> malformed[1:2])
    @test malformed_range isa StringIndexError
    @test collect(codeunits(malformed_range.string)) == UInt8[0xf0, 0x80, 0x80, 0x80]
    @test malformed_range.index == 2

    malformed_step_range = caught_11572(() -> malformed[1:1:4])
    @test malformed_step_range isa StringIndexError
    @test collect(codeunits(malformed_step_range.string)) == UInt8[0xf0, 0x80, 0x80, 0x80]
    @test malformed_step_range.index == 2

    # Range processing must fail at the first invalid requested index without
    # allocating a vector proportional to the caller's range (#11640).
    @test caught_11572(() -> "a"[1:10_000_000]) isa BoundsError
    @test caught_11572(() -> "a"[1:typemax(Int)]) isa BoundsError

    malformed_indexed = caught_11572(() -> malformed[[2]])
    @test malformed_indexed isa StringIndexError
    @test collect(codeunits(malformed_indexed.string)) == UInt8[0xf0, 0x80, 0x80, 0x80]
    @test malformed_indexed.index == 2

    standalone_continuation = String(UInt8[0x80, 0x61])
    @test collect(codeunits(string(standalone_continuation[1]))) == UInt8[0x80]

    # Materialize the outer exception before the catch body can raise a second
    # StringIndexError; rethrow() must retain the outer receiver (#11632).
    outer = "é"
    inner = "à"
    nested_rethrow = caught_11572() do
        try
            outer[2]
        catch
            try
                inner[2]
            catch
            end
            rethrow()
        end
    end
    @test nested_rethrow isa StringIndexError
    @test nested_rethrow.string === outer
    @test nested_rethrow.index == 2
end

@testset "Unified string-index validation matrix (Issue #11621)" begin
    ascii = "abc"
    multibyte = "aéz"

    valid_cases = [
        (() -> ascii[1], 'a'),
        (() -> ascii[3], 'c'),
        (() -> multibyte[1], 'a'),
        (() -> multibyte[4], 'z'),
        (() -> ascii[[1, 3]], "ac"),
        (() -> multibyte[[2, 4]], "éz"),
        (() -> ascii[1:3], "abc"),
        (() -> multibyte[2:4], "éz"),
    ]
    for (load, expected) in valid_cases
        @test load() == expected
    end

    invalid_boundary_cases = [
        () -> multibyte[3],
        () -> multibyte[[3]],
        () -> multibyte[3:4],
        () -> multibyte[2:3],
    ]
    for load in invalid_boundary_cases
        err = caught_11572(load)
        @test err isa StringIndexError
        @test err.string === multibyte
        @test err.index == 3
    end

    out_of_bounds_cases = [
        () -> multibyte[0],
        () -> multibyte[5],
        () -> multibyte[[5]],
        () -> multibyte[2:5],
    ]
    for load in out_of_bounds_cases
        @test caught_11572(load) isa BoundsError
    end
end

@testset "Base String helpers use character-start endpoints (Issues #11624/#11638)" begin
    @test lastindex("é") == 1
    @test lastindex("aé") == 2
    @test lastindex("éa") == 3
    @test lowercasefirst("Aé") == "aé"
    @test uppercasefirst("aé") == "Aé"
    @test lstrip(" é ") == "é "
    @test rstrip("é ") == "é"
    @test strip(" é ") == "é"
    @test replace("xé", "x" => "y", "q" => "z") == "yé"
end

true
