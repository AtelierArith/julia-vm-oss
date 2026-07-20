using Test

struct HOFPrintFoo8878 end

Base.show(io::IO, ::HOFPrintFoo8878) = print(io, "HOFPrintFoo8878!")

@testset "print/println as function values dispatch user show (Issue #8878)" begin
    x = HOFPrintFoo8878()

    @test sprint(print, x) == "HOFPrintFoo8878!"
    @test sprint(println, x) == "HOFPrintFoo8878!\n"
    @test sprint(show, x) == "HOFPrintFoo8878!"

    f = print
    buf = IOBuffer()
    f(buf, x)
    @test String(take!(buf)) == "HOFPrintFoo8878!"

    g = println
    line_buf = IOBuffer()
    g(line_buf, x)
    @test String(take!(line_buf)) == "HOFPrintFoo8878!\n"
end

true
