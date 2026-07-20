using StaticArrays

function svector_mutable_temp_contract_8507()
    work = similar(SVector{2,Float64})
    work[1] = 3.0
    work[2] = 4.0

    v = SVector(work)
    tv = SVector{2,Float64}(work)

    selected = [0.0, 0.0, 0.0]
    selected[[1, 3]] .= 2.5

    gm_work = similar(SVector{3,Float64})
    gm_work .= 0.0
    gm_work[[1, 3]] .= 1.25
    gm = SVector(gm_work)

    ok_work = length(work) == 2 && work[1] == 3.0 && work[2] == 4.0
    ok_svector = v isa SVector{2,Float64} && v[1] == 3.0 && v[2] == 4.0
    ok_typed = tv isa SVector{2,Float64} && tv[1] == 3.0 && tv[2] == 4.0
    ok_dot_index = selected == [2.5, 0.0, 2.5]
    ok_genz_malik_temp = gm isa SVector{3,Float64} &&
        gm[1] == 1.25 &&
        gm[2] == 0.0 &&
        gm[3] == 1.25

    return ok_work && ok_svector && ok_typed && ok_dot_index && ok_genz_malik_temp
end

svector_mutable_temp_contract_8507()
