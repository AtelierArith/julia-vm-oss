using HCubature
import QuadGK

function hcubature_nonfloat_endpoints_contract_8541()
    x32, w32, wg32 = QuadGK.cachedrule(Float32, 7)
    ok_rule32 = x32 isa Vector{Float32} &&
        w32 isa Vector{Float32} &&
        wg32 isa Vector{Float32} &&
        length(x32) == 8 &&
        length(w32) == 8 &&
        length(wg32) == 4 &&
        x32[end] == 0.0f0

    hq32, he32 = hquadrature(cos, Float32(0), Float32(1))
    ok_hquadrature32 = hq32 isa Float32 &&
        abs(hq32 - Float32(sin(1.0))) < Float32(1e-6) &&
        he32 >= 0.0

    hqi, hei = hquadrature(cos, 0, 1)
    ok_hquadrature_int = hqi isa Float64 &&
        abs(hqi - sin(1.0)) < 1e-10 &&
        hei >= 0.0

    vbig, ebig = hcubature(
        x -> x[1] * x[2] * x[3],
        (big"0.0", big"0.0", big"0.0"),
        (big"1.0", big"1.0", big"1.0"),
    )
    ok_hcubature_big = abs(vbig - big"0.125") < big"1e-20" && ebig >= big"0.0"

    return ok_rule32 && ok_hquadrature32 && ok_hquadrature_int && ok_hcubature_big
end

hcubature_nonfloat_endpoints_contract_8541()
