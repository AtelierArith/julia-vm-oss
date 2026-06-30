using QuadGK

function quadgk_inplace_batch_contract_8289()
    result = [0.0, 0.0]
    function vf!(y, x)
        y[1] = x
        y[2] = x^2
        y
    end

    vi, ei = quadgk!(vf!, result, 0.0, 1.0, rtol=1e-3)
    ok_inplace = abs(vi[1] - 0.5) < 1e-12 &&
        abs(vi[2] - 1 / 3) < 1e-12 &&
        ei < 1e-10 &&
        abs(result[1] - 0.5) < 1e-12 &&
        abs(result[2] - 1 / 3) < 1e-12

    function bf!(y, x)
        for i in eachindex(x)
            y[i] = x[i]^2
        end
        y
    end

    bi = BatchIntegrand(bf!, Float64[])
    ok_partial_isa = bi isa BatchIntegrand{Float64, Nothing}

    vb, eb = quadgk(bi, 0.0, 1.0, rtol=1e-3)
    ok_batch = abs(vb - 1 / 3) < 1e-12 && eb < 1e-10

    vb2, eb2 = quadgk(bi, 0.0, 1.0, maxevals=1)
    ok_batch_kw = abs(vb2 - 1 / 3) < 1e-12 && eb2 < 1e-10

    ok_inplace && ok_partial_isa && ok_batch && ok_batch_kw
end

quadgk_inplace_batch_contract_8289()
