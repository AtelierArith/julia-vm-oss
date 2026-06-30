# Issue #5675: a Dict is a mutable reference object — passing it to a function
# and mutating it inside MUST be observed by the caller (pass-by-reference),
# matching upstream Julia. Previously `Value::Dict` was an owned box that was
# deep-copied when bound to a parameter, so mutations were lost (and mergewith /
# accumulator patterns produced wrong results).
#
# NOTE: assertions use per-key checks rather than whole-Dict `==` to avoid an
# unrelated pre-existing Dict-`==` compile limitation.

using Test

# Mutating a Dict argument propagates to the caller.
function fill_it!(d)
    d["x"] = "hello"
    d["y"] = "world"
    return nothing
end
d = Dict{String,String}()
fill_it!(d)
@test length(d) == 2
@test d["x"] == "hello"
@test d["y"] == "world"

# mergewith combines values with a function (the headline fallout).
m = mergewith(+, Dict(1 => 2, 2 => 3), Dict(1 => 10, 3 => 4))
@test length(m) == 3
@test m[1] == 12
@test m[2] == 3
@test m[3] == 4

# mergewith! accumulator pattern through a function argument.
function accumulate!(d, items)
    for k in items
        mergewith!(+, d, Dict(k => 1))
    end
    return nothing
end
counts = Dict{String,Int64}("a" => 0)
accumulate!(counts, ["a", "b", "a", "c", "b", "a"])
@test counts["a"] == 3
@test counts["b"] == 2
@test counts["c"] == 1

# merge!(dest, src) mutates dest in place — observed by the caller.
function merge_into!(dest, src)
    merge!(dest, src)
    return nothing
end
base = Dict("p" => 1.0)
merge_into!(base, Dict("q" => 2.0))
@test length(base) == 2
@test base["q"] == 2.0

# copy(d) and (non-bang) merge(a, b) produce INDEPENDENT dicts: mutating the
# result must NOT affect the source.
orig = Dict("k" => 1.0)
dup = copy(orig)
dup["k"] = 99.0
dup["new"] = 5.0
@test orig["k"] == 1.0
@test !haskey(orig, "new")
@test dup["k"] == 99.0

a = Dict("a" => 1.0)
mg = merge(a, Dict("b" => 2.0))
mg["a"] = 100.0
@test a["a"] == 1.0
@test length(a) == 1

# Aliasing: two bindings to the same dict observe each other's mutations.
x = Dict("v" => 1.0)
y = x
y["v"] = 42.0
@test x["v"] == 42.0

true
