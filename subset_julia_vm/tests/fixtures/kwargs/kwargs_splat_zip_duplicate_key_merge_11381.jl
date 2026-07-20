using Test

# Keyword splatting must dispatch through real `merge(::NamedTuple, source)`
# multiple dispatch so Base's `merge(::NamedTuple, ::Iterators.Zip)`
# duplicate-key validation actually runs (Issue #11381): a zip with duplicate
# keys must raise upstream's `ErrorException`, not silently keep the last
# value.

f(; options...) = options

@testset "kwargs splat zip duplicate-key merge validation (Issue #11381)" begin
    err = nothing
    try
        f(; zip((:a, :a), (1, 2))...)
    catch caught
        err = caught
    end
    @test err isa ErrorException
    @test sprint(showerror, err) == "duplicate field name in NamedTuple: \"a\" is not unique"
end

@testset "kwargs splat zip without duplicate keys still merges (regression)" begin
    kw = f(; zip((:a, :b), (1, 2))...)
    @test kw[:a] == 1
    @test kw[:b] == 2
end

true
