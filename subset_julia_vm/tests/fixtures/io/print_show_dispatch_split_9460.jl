using Test

struct PrintShowSplit9460
    x::Int
end

Base.print(io::IO, p::PrintShowSplit9460) = print(io, "print-", p.x)
Base.show(io::IO, p::PrintShowSplit9460) = print(io, "show-", p.x)

@testset "print and show dispatch stay separate (Issue #9460)" begin
    p = PrintShowSplit9460(7)
    @test sprint(print, p) == "print-7"
    @test string(p) == "print-7"
    @test "$p" == "print-7"
    @test sprint(show, p) == "show-7"
    @test repr(p) == "show-7"
end

true
