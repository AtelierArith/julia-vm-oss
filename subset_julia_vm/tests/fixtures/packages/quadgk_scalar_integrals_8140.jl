using QuadGK

function quadgk_scalar_integrals_contract_8140()
    i1, e1 = quadgk(x -> x^2, 0.0, 1.0)
    ok_poly = abs(i1 - 1 / 3) < 1e-14 && e1 < 1e-12

    i2, e2 = quadgk(sin, 0.0, 1.0)
    ok_sin = abs(i2 - (1 - cos(1.0))) < 1e-14 && e2 < 1e-12

    x, w, wg = QuadGK.cachedrule(Float64, 7)
    ok_rule = length(x) == 8 && length(w) == 8 && length(wg) == 4 && x[end] == 0.0

    gx, gw = QuadGK.gauss(Float64, 3)
    ok_gauss_unit = length(gx) == 3 &&
        abs(gx[1] + 0.7745966692414834) < 1e-14 &&
        gx[2] == 0.0 &&
        abs(gx[3] - 0.7745966692414834) < 1e-14 &&
        abs(gw[1] - 0.5555555555555556) < 1e-14 &&
        abs(gw[2] - 0.8888888888888888) < 1e-14 &&
        abs(gw[3] - 0.5555555555555556) < 1e-14

    rx, rw = QuadGK.gauss(Float64, 3, 0.0, 2.0)
    ok_gauss_rescaled = abs(rx[1] - 0.2254033307585166) < 1e-14 &&
        rx[2] == 1.0 &&
        abs(rx[3] - 1.7745966692414834) < 1e-14 &&
        abs(rw[1] - 0.5555555555555556) < 1e-14 &&
        abs(rw[2] - 0.8888888888888888) < 1e-14 &&
        abs(rw[3] - 0.5555555555555556) < 1e-14

    kx, kw, kgw = QuadGK.kronrod(Float64, 3)
    ok_kronrod = length(kx) == 4 &&
        length(kw) == 4 &&
        length(kgw) == 2 &&
        abs(kx[1] + 0.9604912687080204) < 1e-14 &&
        abs(kx[2] + 0.7745966692414834) < 1e-14 &&
        abs(kx[3] + 0.4342437493468026) < 1e-14 &&
        kx[4] == 0.0 &&
        abs(kw[1] - 0.10465622602646699) < 1e-14 &&
        abs(kw[2] - 0.26848808986833345) < 1e-14 &&
        abs(kw[3] - 0.4013974147759622) < 1e-14 &&
        abs(kw[4] - 0.45091653865847414) < 1e-14 &&
        abs(kgw[1] - 0.5555555555555556) < 1e-14 &&
        abs(kgw[2] - 0.8888888888888885) < 1e-14

    ok_poly && ok_sin && ok_rule && ok_gauss_unit && ok_gauss_rescaled && ok_kronrod
end

quadgk_scalar_integrals_contract_8140()
