# Issue #6731: the Value::Dict Rust carrier was removed; every Dict is a
# pure-Julia Dict{K,V} struct and the full public API dispatches through its
# methods (no NewDict*/_dict_* builtins). Verified vs julia 1.12, including the
# #6584 Any-typed empty! trap and that Set delete!/empty! still work.

using Test

@testset "public Dict API is pure struct dispatch (Issue #6731)" begin
    d = Dict("a" => 1, "b" => 2)
    @test typeof(d) == Dict{String,Int64}
    @test d["a"] == 1
    d["c"] = 3
    @test d["c"] == 3 && length(d) == 3
    @test haskey(d, "b") && !haskey(d, "zz")
    @test get(d, "b", -1) == 2 && get(d, "zz", -1) == -1
    @test getkey(d, "a", "none") == "a"
    @test eltype(d) == Pair{String,Int64} && keytype(d) == String && valtype(d) == Int64
    @test get!(d, "q", 9) == 9 && d["q"] == 9
    merge!(d, Dict("m" => 5))
    @test d["m"] == 5
    @test pop!(d, "q") == 9 && !haskey(d, "q")
    delete!(d, "a")
    @test !haskey(d, "a")
    @test sort(collect(keys(d))) == ["b", "c", "m"]
    @test sort(collect(values(d))) == [2, 3, 5]
    empty!(d)
    @test length(d) == 0
end

@testset "dict construction forms (Issue #6731)" begin
    @test typeof(Dict{Int,Int}()) == Dict{Int64,Int64}
    @test typeof(Dict(i => i^2 for i in 1:3)) == Dict{Int64,Int64}
    cnt = 0
    for (k, v) in Dict(:x => 10, :y => 20)
        cnt += v
    end
    @test cnt == 30
end

@testset "Any-typed binding dispatches to pure Dict (#6584, Issue #6731)" begin
    function mutate(x)
        x[:new] = 7
        delete!(x, :a)
        get!(x, :z, 3)
        return (haskey(x, :a), x[:new], x[:z], length(x))
    end
    @test mutate(Dict(:a => 1, :b => 2)) == (false, 7, 3, 3)
    doempty(x) = (empty!(x); length(x))
    @test doempty(Dict(:p => 1, :q => 2)) == 0
end

@testset "Set delete!/empty! unaffected by Dict carrier removal (Issue #6731)" begin
    s = Set([1, 2, 3])
    delete!(s, 2)
    @test sort(collect(s)) == [1, 3] && length(s) == 2
    empty!(s)
    @test length(s) == 0
end

true
