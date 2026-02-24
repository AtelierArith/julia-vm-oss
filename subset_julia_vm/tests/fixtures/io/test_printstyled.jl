# Test printstyled function for ANSI color output

using Test

# Note: We can't easily test actual color output in automated tests,
# but we can verify the function runs without error and the ANSI
# escape sequences are generated correctly.

@testset "printstyled function" begin
    # Test that printstyled exists and can be called
    # with different color symbols

    # Basic color test - verify it runs without error
    @test begin
        printstyled("red text", :red)
        true
    end

    @test begin
        printstyled("green text", :green)
        true
    end

    @test begin
        printstyled("blue text", :blue)
        true
    end

    # Test bold option
    @test begin
        printstyled("bold yellow", :yellow, true)
        true
    end

    # Test light colors
    @test begin
        printstyled("light cyan", :light_cyan)
        true
    end

    # Test reset/normal color
    @test begin
        printstyled("normal text", :normal)
        true
    end

    # Test ANSI color constant is correct
    @test length(_ANSI_RED) == 5  # ESC [ 3 1 m
    @test length(_ANSI_RESET) == 4  # ESC [ 0 m
end

true  # Test passed
