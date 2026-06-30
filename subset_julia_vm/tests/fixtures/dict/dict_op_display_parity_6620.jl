# Issue #6620: struct-backed Dict{K,V} public operations and compact display
# should match upstream-visible behavior after public construction routes to the
# pure-Julia struct.

using Test

function dict_view_parity()
    d = Dict("a" => 1, "b" => 2)
    ks = keys(d)
    vs = values(d)
    return !isa(ks, Array) &&
           !isa(vs, Array) &&
           length(ks) == length(d) &&
           length(vs) == length(d) &&
           "a" in ks &&
           2 in vs &&
           length(collect(ks)) == 2 &&
           length(collect(vs)) == 2
end

function dict_filter_membership_parity()
    d = Dict("a" => 1, "b" => 2)
    f = filter(p -> p.second > 1, d)
    d2 = copy(d)
    filter!(p -> p.second > 1, d2)
    return ("a" => 1) in d &&
           !(("a" => 9) in d) &&
           f == Dict("b" => 2) &&
           d2 == Dict("b" => 2) &&
           length(d) == 2 &&
           length(d2) == 1
end

function dict_equality_hash_display_parity()
    d1 = Dict("a" => 1, "b" => 2)
    d2 = Dict("b" => 2, "a" => 1)
    d3 = Dict("a" => 1, "b" => 3)
    one = Dict("a" => 1)
    return d1 == d2 &&
           !(d1 == d3) &&
           isequal(d1, d2) &&
           hash(d1) == hash(d2) &&
           repr(one) == "Dict(\"a\" => 1)" &&
           string(one) == repr(one)
end

function dict_reference_and_mixed_key_parity()
    d = Dict("a" => 1)
    mutate!(h) = (h["b"] = 2; h)
    same = mutate!(d)

    mixed = Dict{Any,Int64}(1 => 1, 1.0 => 2, :x => 3, String => 4)
    rehashed = Dict{String,Int64}()
    i = 1
    while i <= 40
        rehashed[string(i)] = i
        i = i + 1
    end

    return same === d &&
           d["b"] == 2 &&
           length(mixed) == 3 &&
           mixed[1] == 2 &&
           mixed[1.0] == 2 &&
           mixed[:x] == 3 &&
           mixed[String] == 4 &&
           length(rehashed) == 40 &&
           rehashed["1"] == 1 &&
           rehashed["40"] == 40
end

all_ok() = dict_view_parity() &&
           dict_filter_membership_parity() &&
           dict_equality_hash_display_parity() &&
           dict_reference_and_mixed_key_parity()

@testset "Dict struct op/display parity (#6620)" begin
    @test dict_view_parity()
    @test dict_filter_membership_parity()
    @test dict_equality_hash_display_parity()
    @test dict_reference_and_mixed_key_parity()
end

all_ok()
