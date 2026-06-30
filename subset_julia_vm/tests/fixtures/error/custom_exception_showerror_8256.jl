using Test

module CustomShowError8256

struct E <: Exception
    x::Int
end

function Base.showerror(io::IO, e::E)
    print(io, "custom ")
    print(io, e.x)
end

end

@testset "package-defined Base.showerror for custom exception" begin
    @test sprint(showerror, CustomShowError8256.E(3)) == "custom 3"
end

true
