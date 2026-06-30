function scalar_array_fused_assignment_contract_8368()
    x = [-0.5, 0.0, 0.5]
    a = 0.0
    s = 1.0
    x .= a .+ (x .+ 1) .* s
    return x == [0.5, 1.0, 1.5]
end

scalar_array_fused_assignment_contract_8368()
