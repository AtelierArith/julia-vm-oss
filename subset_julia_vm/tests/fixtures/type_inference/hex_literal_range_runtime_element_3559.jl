# Issue #3559: hex / binary / octal integer literals encode their bit width in
# the source form (`0x01` is `UInt8`, `0x0001` is `UInt16`, …). Ranges built
# from such literals must preserve that element type at runtime — both
# `typeof(range)` and the loop variable should reflect the literal's typed
# width, not widen to `Int64` / `UnitRange{Int64}`.
using Test

# Hard assertions that must hold for the fixture to pass — these `@assert`s
# raise on failure (returning a non-true value from the script) so the test
# runner detects a regression even though `@test` failures alone wouldn't.

# Plain hex / binary / octal literal types.
@assert typeof(0x01) === UInt8
@assert typeof(0x0001) === UInt16
@assert typeof(0x00000001) === UInt32
@assert typeof(0x0000000000000001) === UInt64
@assert typeof(0b1) === UInt8
@assert typeof(0b1_00000000) === UInt16
@assert typeof(0o7) === UInt8
@assert typeof(0o400) === UInt16

# Range types preserve the typed-literal width.
@assert typeof(0x01:0x05) === UnitRange{UInt8}
@assert typeof(0x0001:0x000a) === UnitRange{UInt16}
@assert typeof(0x00000001:0x0000000a) === UnitRange{UInt32}
@assert typeof(0x0000000000000001:0x000000000000000a) === UnitRange{UInt64}

# Iteration yields elements of the typed width.
let observed = String[]
    for x in 0x01:0x03
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt8", "UInt8", "UInt8"]
end

let observed = String[]
    for x in 0x0001:0x0003
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt16", "UInt16", "UInt16"]
end

let observed = String[]
    for x in 0x00000001:0x00000003
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt32", "UInt32", "UInt32"]
end

let observed = String[]
    for x in 0x0000000000000001:0x0000000000000003
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt64", "UInt64", "UInt64"]
end

# Plain integer ranges still default to Int64.
@assert typeof(1:3) === UnitRange{Int64}
@assert typeof(first(1:3)) === Int64

@testset "Issue #3559 hex literal range element types" begin
    # Plain hex literal types.
    @test typeof(0x01) === UInt8
    @test typeof(0x0001) === UInt16
    @test typeof(0x00000001) === UInt32
    @test typeof(0x0000000000000001) === UInt64

    # Binary and octal literals follow the same width rules.
    @test typeof(0b1) === UInt8
    @test typeof(0b1_00000000) === UInt16
    @test typeof(0o7) === UInt8
    @test typeof(0o400) === UInt16

    # ── Hex ranges of varying widths ─────────────────────────────────────────
    @test typeof(0x01:0x05) === UnitRange{UInt8}
    @test typeof(0x0001:0x000a) === UnitRange{UInt16}
    @test typeof(0x00000001:0x0000000a) === UnitRange{UInt32}
    @test typeof(0x0000000000000001:0x000000000000000a) === UnitRange{UInt64}

    # Iteration variable preserves the typed element type.
    for x in 0x01:0x03
        @test typeof(x) === UInt8
    end
    for x in 0x0001:0x0003
        @test typeof(x) === UInt16
    end
    for x in 0x00000001:0x00000003
        @test typeof(x) === UInt32
    end

    # Plain integer ranges still default to Int64.
    @test typeof(1:3) === UnitRange{Int64}
end

true
