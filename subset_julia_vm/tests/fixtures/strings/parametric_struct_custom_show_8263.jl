using Test

module CustomShowParametric8263
import Base: show

struct A{T}
    x::T
end

function show(io::IO, a::A{T}) where T
    print(io, "custom")
end
end

@testset "custom show for parametric struct" begin
    a = CustomShowParametric8263.A{Int}(1)

    @test sprint(show, a) == "custom"
    @test string(a) == "custom"
end

true
