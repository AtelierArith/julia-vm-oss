# User keys/values/pairs extensions must dispatch correctly even when Base is
# served from the precompiled cache (Issue #8602 retires the #4671
# disable-the-whole-Base-cache fallback in favor of the #8555 cached-candidate
# refresh + program-local-type anchoring gate).
# Verified against upstream julia 1.12.

import Base: keys, values, pairs

struct SmallMap8602
    ks::Vector{Symbol}
    vs::Vector{Int}
end

keys(m::SmallMap8602) = m.ks
values(m::SmallMap8602) = m.vs
pairs(m::SmallMap8602) = [k => v for (k, v) in zip(m.ks, m.vs)]

m = SmallMap8602([:a, :b, :c], [1, 2, 3])

ok = true

# Direct dispatch to the user methods.
ok = ok && keys(m) == [:a, :b, :c]
ok = ok && values(m) == [1, 2, 3]
ok = ok && pairs(m) == [:a => 1, :b => 2, :c => 3]

# User methods compose with Base generics.
ok = ok && collect(keys(m)) == [:a, :b, :c]
ok = ok && sum(values(m)) == 6
ok = ok && length(pairs(m)) == 3

total = 0
for (k, v) in pairs(m)
    global total += v
end
ok = ok && total == 6

# Passing the extended generic function as a value must also reach the
# user method.
ok = ok && map(keys, [m, m]) == [[:a, :b, :c], [:a, :b, :c]]

# Base-known receivers keep the Base methods next to the user extension.
d = Dict(:x => 10)
ok = ok && collect(keys(d)) == [:x]
ok = ok && collect(values(d)) == [10]
ok = ok && collect(pairs(d)) == [:x => 10]
nt = (p = 1, q = 2)
ok = ok && keys(nt) == (:p, :q)
ok = ok && values(nt) == (1, 2)

ok
