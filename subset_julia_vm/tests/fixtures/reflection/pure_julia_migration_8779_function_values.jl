# Issue #8779: public Ref / composition / nonmissingtype / deepcopy names should
# be ordinary callable Julia values, not only direct Rust builtin aliases.

function pj8779_inc(x)
    return x + 1
end

function pj8779_double(x)
    return x * 2
end

ref_ctor = Ref
r = ref_ctor(10)
if r[] != 10
    error("Ref function value construction failed")
end

ref_get = getindex
if ref_get(r) != 10
    error("getindex function value failed")
end
r[] = 11
if ref_get(r) != 11
    error("getindex after ref mutation failed")
end

compose_op = ∘
h = compose_op(pj8779_inc, pj8779_double)
if h(5) != 11
    error("composition function value failed")
end

nm = nonmissingtype
if nm(Union{Int64, Missing}) !== Int64
    error("nonmissingtype function value failed")
end

copy_fn = deepcopy
arr = Any[Ref(1), Ref(2)]
dup = copy_fn(arr)
dup[1][] = 99
if arr[1][] != 1
    error("deepcopy aliased the original ref")
end
if dup[1][] != 99
    error("deepcopy result ref was not mutable")
end

true
