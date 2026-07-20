# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: reflection/args_constant.jl =====
# Test ARGS constant
# ARGS should be a Vector{String} (command line arguments)


@testset "ARGS constant: command-line arguments array (Issue #340)" begin

    result = true

    # Check that ARGS is an Array
    if !(typeof(ARGS) <: AbstractArray)
        result = false
    end

    # For SubsetJuliaVM, ARGS is always empty (no CLI args passed)
    # But it should still be a valid array
    if length(ARGS) != 0
        # This is OK - ARGS might have values in Julia REPL
        # For SubsetJuliaVM we expect it to be empty
    end

    @test (result)
end

# ===== source: reflection/env_constant.jl =====
# Test ENV constant
# ENV should be a Dict{String,String} containing environment variables


@testset "ENV constant: environment variable dictionary (Issue #340)" begin

    result = true

    # Check that ENV is a Dict
    if !(typeof(ENV) <: AbstractDict)
        result = false
    end

    # ENV should have some entries (at least PATH or HOME on most systems)
    if length(ENV) == 0
        # This might be valid in some sandboxed environments,
        # but typically ENV has at least some variables
        # We don't fail here as it depends on the execution environment
    end

    # Test haskey function on ENV
    # PATH is almost always present on Unix systems
    # HOME is common on macOS/Linux, USERPROFILE on Windows
    has_some_var = haskey(ENV, "PATH") || haskey(ENV, "HOME") || haskey(ENV, "USER")

    # We don't fail if no vars found - could be sandboxed environment
    # Just test that haskey works without error

    # Test that we can iterate over ENV keys (if any exist)
    key_count = 0
    for key in keys(ENV)
        key_count = key_count + 1
        if key_count >= 3
            break  # Just test a few
        end
    end

    @test (result)
end

# ===== source: reflection/native_word_aliases_6097_6105.jl =====

@testset "native word aliases (Issues #6097, #6105)" begin
    native_int = Sys.WORD_SIZE == 32 ? Int32 : Int64
    native_uint = Sys.WORD_SIZE == 32 ? UInt32 : UInt64

    @test Int === native_int
    @test UInt === native_uint

    @test typeof(Int(7)) === native_int
    @test typeof(UInt(7)) === native_uint

    @test Vector{Int} === Vector{native_int}
    @test Vector{UInt} === Vector{native_uint}
    @test Tuple{Int, UInt} === Tuple{native_int, native_uint}
end

# ===== source: reflection/program_file_constant.jl =====
# Test PROGRAM_FILE constant
# PROGRAM_FILE should be a String (path to the running script)


@testset "PROGRAM_FILE constant: path to running script (Issue #340)" begin

    result = true

    # Check that PROGRAM_FILE is a String
    if !(typeof(PROGRAM_FILE) <: AbstractString)
        result = false
    end

    # For SubsetJuliaVM in embedded mode, PROGRAM_FILE is empty string
    # For Julia running a script, it would contain the script path
    # We just check that it's a valid String (either empty or with content)

    @test (result)
end

# ===== source: reflection/sys_word_size_6096.jl =====

@testset "Sys.WORD_SIZE module binding (Issue #6096)" begin
    @test Sys.WORD_SIZE == 32 || Sys.WORD_SIZE == 64
    @test typeof(Sys.WORD_SIZE) === Int
    @test isdefined(Sys, :WORD_SIZE)
    @test getfield(Sys, :WORD_SIZE) == Sys.WORD_SIZE
end

# ===== source: reflection/version_constant.jl =====
# Test VERSION constant and VersionNumber type
# VERSION is now a VersionNumber struct with major, minor, patch fields


@testset "VERSION constant: global version string (Issue #340)" begin

    result = true

    # Test that VERSION is a VersionNumber
    @assert typeof(VERSION) == VersionNumber

    # Test that we can access version fields (check they are integers >= 0)
    @assert VERSION.major >= 0
    @assert VERSION.minor >= 0
    @assert VERSION.patch >= 0

    # Test VersionNumber constructors
    v1 = VersionNumber(1, 2, 3)
    @assert v1.major == 1
    @assert v1.minor == 2
    @assert v1.patch == 3

    # Test 2-arg constructor (patch defaults to 0)
    v2 = VersionNumber(2, 5)
    @assert v2.major == 2
    @assert v2.minor == 5
    @assert v2.patch == 0

    # Test 1-arg constructor (minor and patch default to 0)
    v3 = VersionNumber(3)
    @assert v3.major == 3
    @assert v3.minor == 0
    @assert v3.patch == 0

    @test (result)
end

true
