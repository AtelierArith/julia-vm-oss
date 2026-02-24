# Test @__LINE__ and @__MODULE__ macros
# These macros return information about the source location at compile time

using Test

@testset "@__LINE__ and @__MODULE__ macros" begin
    # @__LINE__ returns the current line number as an integer
    # The exact line depends on where the macro is called
    line1 = @__LINE__
    @test typeof(line1) == Int64
    @test line1 > 0

    # Consecutive lines should have increasing line numbers
    line2 = @__LINE__
    @test line2 > line1

    # @__MODULE__ returns the current module
    mod = @__MODULE__
    # Check that it's a Module type by checking its string representation
    mod_str = string(mod)
    @test mod_str == "Main"
end

true
