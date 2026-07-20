using Test

abstract type AnimalShow8823 end

struct DogShow8823 <: AnimalShow8823
    name::String
end

Base.show(io::IO, a::AnimalShow8823) = print(io, "Animal(", a.name, ")")

@testset "show on abstract supertype covers concrete subtype (Issue #8823)" begin
    d = DogShow8823("Rex")
    @test string(d) == "Animal(Rex)"
    @test sprint(show, d) == "Animal(Rex)"

    buf = IOBuffer()
    print(buf, d)
    @test String(take!(buf)) == "Animal(Rex)"
end

@testset "Base collection displays dispatch through abstract and union show methods (Issue #8823)" begin
    d = Dict(:x => 10)

    @test repr(keys(d)) == "[:x]"
    @test string(keys(d)) == "[:x]"
    buf_keys = IOBuffer()
    print(buf_keys, keys(d))
    @test String(take!(buf_keys)) == "[:x]"

    @test repr(values(d)) == "[10]"
    @test string(values(d)) == "[10]"
    buf_values = IOBuffer()
    print(buf_values, values(d))
    @test String(take!(buf_values)) == "[10]"

    s = Set([:a])
    @test repr(s) == "Set([:a])"
    @test string(s) == "Set([:a])"
    buf_set = IOBuffer()
    print(buf_set, s)
    @test String(take!(buf_set)) == "Set([:a])"
end

true
