# get!(dict, key, default) and get!(f, dict, key) must persist the inserted
# entry to the bound dict variable, matching upstream Julia (Issue #5225).
#
# Before the fix the `get!` compile path loaded the dict by value and ran the
# DictGetBang builtin but never stored the mutated dict back, so the inserted
# entry was lost. This mirrors how `pop!`/`merge!` persist their mutations.
#
# Reference: julia/base/abstractdict.jl (get!(t, key, default) delegates to the
# thunk form get!(() -> default, t, key); the callable runs only when the key
# is absent).

using Test

# Counting idiom with String keys: get! must persist the inserted 0 across
# iterations for the running count to be correct.
function count_chars(s)
    counts = Dict{String,Int}()
    for c in s
        k = string(c)
        counts[k] = get!(counts, k, 0) + 1
    end
    return counts
end

@testset "get! write-back (Issue #5225)" begin
    # dict-first: absent key inserts default and persists to the bound variable
    di = Dict{Int,Int}(1 => 5)
    @test get!(di, 2, 9) == 9
    @test haskey(di, 2)
    @test di[2] == 9
    @test length(di) == 2

    # dict-first: present key returns existing value without overwriting
    @test get!(di, 1, 100) == 5
    @test di[1] == 5
    @test length(di) == 2

    # String keys
    ds = Dict{String,Int}()
    @test get!(ds, "a", 1) == 1
    @test haskey(ds, "a")
    @test ds["a"] == 1
    # present key: no overwrite
    @test get!(ds, "a", 77) == 1
    @test ds["a"] == 1

    # Symbol keys, Float64 values
    df = Dict{Symbol,Float64}()
    @test get!(df, :x, 3.5) == 3.5
    @test df[:x] == 3.5
    @test length(df) == 1

    # Counting idiom: relies on get! persisting between loop iterations
    counts = count_chars("hello")
    @test counts["l"] == 2
    @test counts["h"] == 1
    @test counts["e"] == 1
    @test counts["o"] == 1
    @test length(counts) == 4

    # Thunk form (lambda): absent key inserts thunk result and persists
    dt = Dict{Int,Int}()
    @test get!(() -> 42, dt, 3) == 42
    @test haskey(dt, 3)
    @test dt[3] == 42
    @test length(dt) == 1

    # Thunk form: present key returns existing value and the thunk is NOT
    # evaluated (an erroring thunk would throw if it ran).
    dt[7] = 70
    @test get!(() -> error("thunk must not run when key is present"), dt, 7) == 70
    @test dt[7] == 70
    @test length(dt) == 2

    # Thunk form with a named function: absent key evaluates the thunk once,
    # inserts the result, and persists it.
    make_default() = 1000
    dn = Dict{String,Int}("present" => 1)
    @test get!(make_default, dn, "present") == 1
    @test dn["present"] == 1
    @test get!(make_default, dn, "absent") == 1000
    @test haskey(dn, "absent")
    @test dn["absent"] == 1000
    @test length(dn) == 2

    # Non-variable receiver: returns the correct value (nothing to write back)
    @test get!(Dict(:a => 1), :b, 2) == 2
    @test get!(Dict(:a => 1), :a, 99) == 1
end

true
