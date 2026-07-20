using HCubature

# Issue #8524: generic n-dimensional cubature beyond the 1-D/2-D smoke slice.
function hcubature_ndim_contract_8524()
    # 3-D MWE from the issue: ∫∫∫ exp(-(x²+y²+z²)) over [0,1]³ = (√π/2 · erf(1))³
    val3, err3 = hcubature(
        x -> exp(-(x[1]^2 + x[2]^2 + x[3]^2)),
        (0.0, 0.0, 0.0),
        (1.0, 1.0, 1.0);
        rtol=1e-6,
    )
    ok_gauss3 = abs(val3 - 0.4165383907322842) < 1e-6 && err3 >= 0

    # 3-D separable cosine with default tolerance: ∫ cos x cos y cos z = sin(1)³
    c3, ce3 = hcubature(x -> cos(x[1]) * cos(x[2]) * cos(x[3]), (0.0, 0.0, 0.0), (1.0, 1.0, 1.0))
    ok_cos3 = abs(c3 - sin(1.0)^3) < 1e-8 && ce3 >= 0

    # 4-D polynomial, exact for the Genz-Malik rule: ∫ (x₁+x₂+x₃+x₄) over [0,1]⁴ = 2
    p4, pe4 = hcubature(x -> x[1] + x[2] + x[3] + x[4], (0.0, 0.0, 0.0, 0.0), (1.0, 1.0, 1.0, 1.0))
    ok_poly4 = abs(p4 - 2.0) < 1e-10 && pe4 >= 0

    # 5-D separable cosine: ∫ ∏ cos(xᵢ) = sin(1)⁵
    c5, ce5 = hcubature(
        x -> cos(x[1]) * cos(x[2]) * cos(x[3]) * cos(x[4]) * cos(x[5]),
        (0.0, 0.0, 0.0, 0.0, 0.0),
        (1.0, 1.0, 1.0, 1.0, 1.0);
        rtol=1e-6,
    )
    ok_cos5 = abs(c5 - sin(1.0)^5) < 1e-6 && ce5 >= 0

    # Vector (not tuple) endpoints in 3-D: ∫ x₁x₂x₃ = 1/8
    v3, _ = hcubature(x -> x[1] * x[2] * x[3], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
    ok_vec3 = abs(v3 - 0.125) < 1e-10

    # Integer endpoints promote to Float64 in 3-D: ∫ (x₁+x₂+x₃) = 1.5
    m3, _ = hcubature(x -> x[1] + x[2] + x[3], (0, 0, 0), (1, 1, 1))
    ok_int3 = abs(m3 - 1.5) < 1e-10

    # initdiv > 1 exercises the n-dimensional initial subdivision
    i3, _ = hcubature(
        x -> cos(x[1]) * cos(x[2]) * cos(x[3]),
        (0.0, 0.0, 0.0),
        (1.0, 1.0, 1.0);
        initdiv=2,
    )
    ok_initdiv3 = abs(i3 - sin(1.0)^3) < 1e-8

    # Buffer reuse in 3-D
    f3 = x -> cos(x[1]) * cos(x[2]) * cos(x[3])
    buf = hcubature_buffer(f3, (0.0, 0.0, 0.0), (1.0, 1.0, 1.0))
    b3, _ = hcubature(f3, (0.0, 0.0, 0.0), (1.0, 1.0, 1.0); rtol=1e-6, buffer=buf)
    ok_buffer3 = abs(b3 - sin(1.0)^3) < 1e-6

    # Genz-Malik evaluation count for a constant integrand: 1 + 4n + 2n(n-1) + 2ⁿ
    _, _, cnt3 = hcubature_count(x -> 1.0, (0.0, 0.0, 0.0), (1.0, 1.0, 1.0))
    _, _, cnt7 = hcubature_count(
        x -> 1.0,
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        (1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0),
    )
    ok_count = cnt3 == 33 && cnt7 == 241

    return ok_gauss3 && ok_cos3 && ok_poly4 && ok_cos5 && ok_vec3 && ok_int3 &&
           ok_initdiv3 && ok_buffer3 && ok_count
end

hcubature_ndim_contract_8524()
