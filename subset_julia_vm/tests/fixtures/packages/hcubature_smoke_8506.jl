using HCubature
using StaticArrays

function hcubature_smoke_contract_8506()
    z, ze = hcubature(x -> 1.7, SVector{0,Float64}(), SVector{0,Float64}())
    ok_0d = z == 1.7 && ze == 0.0

    q, qe = hquadrature(cos, 0.0, 1.0)
    ok_1d = abs(q - sin(1.0)) < 1e-12 && qe == 0.0

    hq, hqe = hcubature(x -> cos(x[1]), (0.0,), (1.0,))
    ok_hcubature_1d = abs(hq - sin(1.0)) < 1e-12 && hqe == 0.0

    i, e = hcubature(x -> cos(x[1]) * cos(x[2]), (0.0, 0.0), (1.0, 1.0))
    ok_2d = abs(i - sin(1.0)^2) < 1e-12 && 0.0 <= e && e < 1e-8

    f = x -> cos(x[1]) * cos(x[2])
    buffer = hcubature_buffer(f, (0.0, 0.0), (1.0, 1.0))
    ib, eb = hcubature(f, (0.0, 0.0), (1.0, 1.0); rtol=1e-3, buffer=buffer)
    ok_buffer = abs(ib - sin(1.0)^2) < 1e-6 && 0.0 <= eb && eb < 1e-4

    q3, qe3 = hquadrature(cos, 0.0, 1.0; initdiv=3)
    i3, e3 = hcubature(f, (0.0, 0.0), (1.0, 1.0); initdiv=3)
    ok_initdiv = abs(q3 - sin(1.0)) < 1e-12 &&
        0.0 <= qe3 && qe3 < 1e-12 &&
        abs(i3 - sin(1.0)^2) < 1e-12 &&
        0.0 <= e3 && e3 < 2e-8

    function normal2_fullspace(t)
        x1 = t[1] / (1 - t[1]^2)
        x2 = t[2] / (1 - t[2]^2)
        j1 = (1 + t[1]^2) / (1 - t[1]^2)^2
        j2 = (1 + t[2]^2) / (1 - t[2]^2)^2
        return exp(-0.5 * (x1 * x1 + x2 * x2)) / (2 * pi) * j1 * j2
    end

    ni, ne = hcubature(normal2_fullspace, (-1.0, -1.0), (1.0, 1.0); rtol=1e-3)
    ok_normal = abs(ni - 1.0) < 2e-5 && 0.0 <= ne && ne < 1e-3

    return ok_0d && ok_1d && ok_hcubature_1d && ok_2d && ok_buffer && ok_initdiv && ok_normal
end

hcubature_smoke_contract_8506()
