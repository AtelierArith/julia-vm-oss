using Test

mutable struct RefWeakBox8990
    x::Int
end

calls = Ref(0)
obj = RefWeakBox8990(41)
wr = WeakRef(obj)

@test typeof(wr) === WeakRef
@test wr.value === obj

finalizer(x -> (calls[] = calls[] + x.x; nothing), obj)
@test finalize(obj) === nothing
@test finalize(obj) === nothing
@test calls[] == 41

function make_weak_ref_8990()
    tmp = RefWeakBox8990(7)
    return WeakRef(tmp)
end

ephemeral = make_weak_ref_8990()
@test !(ephemeral.value === nothing)

wkd = WeakKeyDict()
key = RefWeakBox8990(5)
setindex!(wkd, "five", key)
@test haskey(wkd, key)
@test get(wkd, key, "missing") == "five"
@test wkd[key] == "five"
@test getindex(wkd, key) == "five"
@test length(wkd) == 1

function weakkeydict_any_lookup_10088(d, k)
    return d[k]
end

function weakkeydict_any_store_10088(d, k, v)
    d[k] = v
    return d[k]
end

@test weakkeydict_any_lookup_10088(wkd, key) == "five"
@test weakkeydict_any_store_10088(wkd, key, "FIVE") == "FIVE"
@test wkd[key] == "FIVE"

wkd_array = WeakKeyDict()
array_key = [1]
wkd_array[array_key] = "array-key"
@test weakkeydict_any_lookup_10088(wkd_array, array_key) == "array-key"
@test weakkeydict_any_store_10088(wkd_array, array_key, "ARRAY-KEY") == "ARRAY-KEY"
@test wkd_array[array_key] == "ARRAY-KEY"

seen = 0
for p in wkd
    @test p.first === key
    @test p.second == "FIVE"
    global seen += 1
end
@test seen == 1

delete!(wkd, key)
@test !haskey(wkd, key)

key2 = RefWeakBox8990(6)
setindex!(wkd, "again", key2)
@test length(wkd) == 1
key2 = nothing
GC.gc()
GC.gc()
@test length(wkd) == 0

# Issue #10088 review: an array-valued key must dispatch through the
# receiver's own getindex/setindex! method rather than being treated as a
# fancy/logical index selector into the receiver itself (the array-shaped
# key would otherwise be misread as "index this array-like target with
# these positions", erroring with "logical indexing requires an Array
# target" for a non-Array receiver like WeakKeyDict).
wkd2 = WeakKeyDict()
akey = [1]

function weakkeydict_array_key_store_10088(d, k, v)
    d[k] = v
    return d
end

function weakkeydict_array_key_lookup_10088(d, k)
    return d[k]
end

weakkeydict_array_key_store_10088(wkd2, akey, "arrval")
@test weakkeydict_array_key_lookup_10088(wkd2, akey) == "arrval"
@test haskey(wkd2, akey)
@test length(wkd2) == 1

true
