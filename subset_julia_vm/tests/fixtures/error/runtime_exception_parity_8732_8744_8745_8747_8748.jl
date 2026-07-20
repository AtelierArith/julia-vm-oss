using Test

@testset "runtime exceptions stay catchable and structured (Issues #8732/#8744/#8745/#8747/#8748)" begin
    arr = Int[]
    try
        pop!(arr)
        @test false
    catch e
        @test typeof(e) === ArgumentError
        @test sprint(showerror, e) == "ArgumentError: array must be non-empty"
    end

    f_kw_8745(; kw) = kw
    try
        f_kw_8745()
        @test false
    catch e
        @test typeof(e) === UndefKeywordError
        @test sprint(showerror, e) == "UndefKeywordError: keyword argument `kw` not assigned"
    end

    d = Dict(1 => 2)
    try
        d[5]
        @test false
    catch e
        @test typeof(e) === KeyError
        @test sprint(showerror, e) == "KeyError: key 5 not found"
    end

    try
        convert(Int64, 1.5)
        @test false
    catch e
        @test typeof(e) === InexactError
        @test fieldnames(typeof(e)) == (:func, :args)
        @test e.args == (Int64, 1.5)
        @test sprint(showerror, e) == "InexactError: Int64(1.5)"
    end

    try
        abs("str")
        @test false
    catch e
        @test typeof(e) === MethodError
        @test occursin("MethodError: no method matching", sprint(showerror, e))
    end
end

true
