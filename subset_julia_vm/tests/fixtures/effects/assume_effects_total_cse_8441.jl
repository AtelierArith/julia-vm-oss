using Test

Base.@assume_effects :total function assumed_total_8441(x)
    return x + 1
end

function assume_effects_total_cse_8441(x)
    a = assumed_total_8441(x)
    b = assumed_total_8441(x)
    return a + b
end

@test assume_effects_total_cse_8441(10) == 22

true
