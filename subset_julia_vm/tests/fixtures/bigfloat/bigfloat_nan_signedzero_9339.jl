# Issue #9339: BigFloat NaN comparisons must be unordered (all false, != true),
# and add/sub must honor the IEEE-754 RNE signed-zero rule (+0 wins; -0 only
# when both effective operands are -0). Verified against julia 1.12.6.
nz = -zero(BigFloat)      # genuine -0.0
pz = zero(BigFloat)       # +0.0
nan = big(NaN)

results = Bool[]

# NaN is unordered: <, <=, >, >= all false; == false; != true.
push!(results, (nz <= nan) == false)
push!(results, (nz < nan) == false)
push!(results, (nz > nan) == false)
push!(results, (nz >= nan) == false)
push!(results, (nan <= nz) == false)
push!(results, (nan < nz) == false)
push!(results, (nan >= pz) == false)
push!(results, (nan > pz) == false)
push!(results, (nz == nan) == false)
push!(results, (nz != nan) == true)
push!(results, (nan == nan) == false)
push!(results, (nan != nan) == true)

# IEEE-754 RNE signed-zero rule for + and -.
# add: -0 only when both operands are -0; every mixed/positive case is +0.
push!(results, string(pz + nz) == "0.0")
push!(results, string(nz + pz) == "0.0")
push!(results, string(nz + nz) == "-0.0")
push!(results, string(pz + pz) == "0.0")
# sub: x - y == x + (-y); -0 only when x is -0 and y is +0.
push!(results, string(pz - nz) == "0.0")
push!(results, string(nz - pz) == "-0.0")
push!(results, string(nz - nz) == "0.0")
push!(results, string(pz - pz) == "0.0")

all(results)
