# Test gensym - generate unique symbol names
# gensym() generates symbols like ##0, ##1, etc.
# gensym("tag") generates symbols like ##tag#0, ##tag#1, etc.

using Test

@testset "gensym() generates unique symbols for macro hygiene" begin

    # Test gensym() without argument
    s1 = gensym()
    s2 = gensym()
    @assert s1 != s2  # Each gensym should be unique

    # Test gensym() with string argument
    s3 = gensym("x")
    s4 = gensym("x")
    @assert s3 != s4  # Even with same tag, symbols should be unique

    # Test gensym() with symbol argument
    s5 = gensym(:loop)
    s6 = gensym(:loop)
    @assert s5 != s6  # Unique even with symbol argument

    # Test that gensym returns a Symbol
    @assert typeof(s1) == Symbol
    @assert typeof(s3) == Symbol
    @assert typeof(s5) == Symbol

    # Test that gensym strings contain expected patterns
    str1 = string(s1)
    @assert occursin("##", str1)

    str3 = string(s3)
    @assert occursin("##x#", str3)

    str5 = string(s5)
    @assert occursin("##loop#", str5)

    # Return success
    @test (1.0) == 1.0
end

true  # Test passed
