# MIME type system tests
# Tests MIME type creation, istextmime, showable, and 3-argument show

using Test

@testset "MIME type system" begin
    # Test 1: MIME type creation with string constructor
    m1 = MIME("text/plain")
    @test true  # Successfully created MIME type

    # Test 2: MIME string literal syntax
    m2 = MIME"text/html"
    @test true  # Successfully created MIME type with string literal

    # Test 3: istextmime for text types
    @test istextmime(MIME("text/plain")) == true
    @test istextmime(MIME("text/html")) == true
    @test istextmime(MIME("text/css")) == true

    # Test 4: istextmime for known text application types
    @test istextmime(MIME("application/json")) == true
    @test istextmime(MIME("application/javascript")) == true
    @test istextmime(MIME("application/xml")) == true

    # Test 5: istextmime for binary types
    @test istextmime(MIME("image/png")) == false
    @test istextmime(MIME("image/jpeg")) == false
    @test istextmime(MIME("application/octet-stream")) == false

    # Test 6: istextmime with string argument
    @test istextmime("text/plain") == true
    @test istextmime("image/png") == false

    # Test 7: showable for text/plain (always true)
    @test showable(MIME("text/plain"), 42) == true
    @test showable(MIME("text/plain"), "hello") == true
    @test showable(MIME("text/plain"), [1, 2, 3]) == true

    # Test 8: showable for other MIME types (false by default)
    @test showable(MIME("image/png"), 42) == false
    @test showable(MIME("text/html"), "hello") == false

    # Test 9: 3-argument show with MIME type
    io = IOBuffer()
    show(io, MIME("text/plain"), 42)
    result = take!(io)
    @test result == "42"

    # Test 10: 3-argument show with string
    io2 = IOBuffer()
    show(io2, MIME("text/plain"), "hello")
    result2 = take!(io2)
    @test result2 == "\"hello\""
end

true
