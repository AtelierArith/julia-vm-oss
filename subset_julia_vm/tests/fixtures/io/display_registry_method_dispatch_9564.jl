using Test

struct DisplayBox9564{T}
    x::T
end

Base.show(io::IO, b::DisplayBox9564{T}) where {T<:Real} = print(io, "show-real-", b.x)
Base.show(io::IO, b::DisplayBox9564{T}) where {T<:Integer} = print(io, "show-integer-", b.x)
Base.print(io::IO, b::DisplayBox9564{T}) where {T<:Integer} = print(io, "print-integer-", b.x)

@testset "display registry uses method specificity for show (Issue #9564)" begin
    i = DisplayBox9564(3)
    f = DisplayBox9564(1.5)

    @test sprint(show, i) == "show-integer-3"
    @test repr(i) == "show-integer-3"
    @test sprint(show, f) == "show-real-1.5"
    @test repr(f) == "show-real-1.5"
end

@testset "print paths prefer print method then method-selected show fallback (Issue #9564)" begin
    i = DisplayBox9564(3)
    f = DisplayBox9564(1.5)

    @test sprint(print, i) == "print-integer-3"
    @test string(i) == "print-integer-3"
    @test "$i" == "print-integer-3"

    @test sprint(print, f) == "show-real-1.5"
    @test string(f) == "show-real-1.5"
    @test "$f" == "show-real-1.5"
end

true
