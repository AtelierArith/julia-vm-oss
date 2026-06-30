using QuadGK

function quadgk_infinite_intervals_contract_8408()
    v_pos, e_pos = quadgk(x -> exp(-x), 0.0, Inf, rtol=1e-3)
    ok_pos = abs(v_pos - 1.0) < 2e-3 && e_pos < 2e-3

    v_neg, e_neg = quadgk(x -> exp(x), -Inf, 0.0, rtol=1e-3)
    ok_neg = abs(v_neg - 1.0) < 2e-3 && e_neg < 2e-3

    ok_pos && ok_neg
end

quadgk_infinite_intervals_contract_8408()
