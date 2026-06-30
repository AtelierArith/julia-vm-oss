using Test

# Regression tests for Issues #3608 and #3609:
# `uppercasefirst` and `lowercasefirst` must handle non-ASCII Latin-1
# letters (e.g. 'é' ↔ 'É', 'ü' ↔ 'Ü'). Previously they checked only the
# ASCII a-z / A-Z range (a single byte) and returned non-ASCII strings
# unchanged.

@testset "uppercasefirst Latin-1 (#3609)" begin
    @test uppercasefirst("élan") == "Élan"
    @test uppercasefirst("über") == "Über"
    @test uppercasefirst("ébc") == "Ébc"
    @test uppercasefirst("ñoño") == "Ñoño"
    @test uppercasefirst("ø") == "Ø"   # single Latin-1 char

    # ASCII regression — must still work
    @test uppercasefirst("hello") == "Hello"
    @test uppercasefirst("a") == "A"

    # Empty + already-uppercase + non-letter unchanged
    @test uppercasefirst("") == ""
    @test uppercasefirst("Hello") == "Hello"
    @test uppercasefirst("123") == "123"

    # Non-Latin-1 (CJK) returned unchanged — full Unicode case mapping is out
    # of scope; matches Julia behavior on chars without case.
    @test uppercasefirst("漢字") == "漢字"
end

@testset "lowercasefirst Latin-1 (#3608)" begin
    @test lowercasefirst("Élan") == "élan"
    @test lowercasefirst("ÉLAN") == "éLAN"
    @test lowercasefirst("Über") == "über"
    @test lowercasefirst("Ñoño") == "ñoño"
    @test lowercasefirst("Ø") == "ø"

    # ASCII regression
    @test lowercasefirst("Hello") == "hello"
    @test lowercasefirst("A") == "a"

    @test lowercasefirst("") == ""
    @test lowercasefirst("hello") == "hello"
    @test lowercasefirst("123") == "123"
end

true
