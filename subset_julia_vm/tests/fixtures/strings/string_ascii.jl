# Test ascii function - validate string contains only ASCII

using Test

@testset "ascii(s) - validate ASCII string" begin

    # === Valid ASCII strings ===
    @assert ascii("hello") == "hello"
    @assert ascii("WORLD") == "WORLD"
    @assert ascii("Hello World!") == "Hello World!"
    @assert ascii("12345") == "12345"
    @assert ascii("") == ""
    @assert ascii(" ") == " "
    @assert ascii("\t\n") == "\t\n"

    # ASCII characters (0-127)
    @assert ascii("ABC abc 123") == "ABC abc 123"
    @assert ascii("!@#\$%^&*()") == "!@#\$%^&*()"

    # All tests passed
    @test (true)
end

true  # Test passed
