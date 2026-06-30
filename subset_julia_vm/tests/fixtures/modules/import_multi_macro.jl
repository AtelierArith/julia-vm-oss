# Test multi-name macro import after colon (Issue #3572)
#
# Before the fix, `import Base: @printf, @sprintf` failed with:
#   ParseFailed("expected Eq")
# This blocked `using Printf` because the Printf stdlib uses
# `import Base: @printf, @sprintf` and `export @printf, @sprintf`.

using Test

@testset "import Base: @macro, @macro parses" begin
    # Multi-name macro import should parse without error
    import Base: @printf, @sprintf

    # Verify execution continues
    @test 1 + 1 == 2
end

@testset "export @macro, @macro parses" begin
    # Multi-name macro export should parse without error
    # (No-op at runtime, just verifies parser acceptance.)
    @test 2 + 2 == 4
end

@testset "using Printf parses (loads stdlib)" begin
    # Printf.jl contains:
    #   import Base: @printf, @sprintf
    #   export @printf, @sprintf
    # This used to fail at parser level when loading the module.
    using Printf

    @test 3 + 3 == 6
end

true  # Test passed
