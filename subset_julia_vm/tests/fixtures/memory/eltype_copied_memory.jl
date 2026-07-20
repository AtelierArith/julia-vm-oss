m = Memory{Int64}(undef, 2)
m[1] = 11
m[2] = 12
c = copy(m)

mf = Memory{Float64}(undef, 2)
mf[1] = 1.5
mf[2] = 2.5
cf = copy(mf)

typeof(c) == Memory{Int64} &&
    eltype(c) == Int64 &&
    c[1] == 11 &&
    c[2] == 12 &&
    typeof(cf) == Memory{Float64} &&
    eltype(cf) == Float64 &&
    cf[1] == 1.5 &&
    cf[2] == 2.5
