using Test

@testset "print/println of unsigned integers use decimal form (Issue #9585)" begin
    widths = (
        (UInt8(2), "2", "0x02"),
        (UInt8(200), "200", "0xc8"),
        (UInt16(513), "513", "0x0201"),
        (UInt32(65537), "65537", "0x00010001"),
        (UInt64(65537), "65537", "0x0000000000010001"),
        (UInt128(65537), "65537", "0x00000000000000000000000000010001"),
    )

    for (value, print_form, show_form) in widths
        buf = IOBuffer()
        print(buf, value)
        @test String(take!(buf)) == print_form

        line = IOBuffer()
        println(line, value)
        @test String(take!(line)) == print_form * "\n"

        shown = IOBuffer()
        show(shown, value)
        @test String(take!(shown)) == show_form

        @test string(value) == print_form
        @test repr(value) == show_form
    end

    # Container element rendering still uses show-form so typed arrays remain
    # parseable and match upstream display.
    @test repr(UInt8[2, 200]) == "UInt8[0x02, 0xc8]"
end

true
