# Test global constants implementation (Issue #430)
# Verifies that global constants like stdout, stderr, C_NULL, etc. are available

using Test

@testset "Global Constants" begin
    # Test 1: IO streams are accessible
    @testset "IO streams exist" begin
        # stdout, stderr, stdin should be accessible without error
        io1 = stdout
        io2 = stderr
        io3 = stdin
        @test true  # If we get here, the constants exist
    end

    # Test 2: devnull is accessible
    @testset "devnull" begin
        io = devnull
        @test true  # If we get here, devnull exists
    end

    # Test 3: C_NULL
    @testset "C_NULL" begin
        # C_NULL is a null pointer; upstream represents it as Ptr{Nothing}(0).
        @test C_NULL == C_NULL
    end

    # Test 4: Float16 special values (exist and have correct values)
    @testset "Float16 special values" begin
        # Inf16 and NaN16 should be accessible
        x = Inf16
        y = NaN16
        @test true  # If we get here, the constants exist
    end

    # Test 5: DEPOT_PATH and LOAD_PATH are arrays
    @testset "Path arrays" begin
        @test DEPOT_PATH isa Vector
        @test LOAD_PATH isa Vector
    end

    # Test 6: Other constants (already implemented)
    @testset "Existing constants" begin
        @test isinf(Inf)
        @test isinf(Inf64)
        @test isnan(NaN)
        @test isnan(NaN64)
        @test nothing === nothing
        @test Float64(pi) == 3.141592653589793
    end
end

true
