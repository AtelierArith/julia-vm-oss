module LineSearches

# Minimal LineSearches compatibility surface for the Optim.jl MVP (Issue #7482).
#
# Upstream LineSearches provides several sophisticated line searches
# (HagerZhang, MoreThuente, StrongWolfe, BackTracking, ...) plus initial-step
# guessers.  The SubsetJuliaVM Optim MVP only needs a single, deterministic
# sufficient-decrease (Armijo) backtracking line search.  The `BackTracking`
# type below carries the tuning parameters consumed by `Optim.GradientDescent`;
# the remaining line-search types are placeholders kept so that user code and
# the Optim source that names them can load.  They fall back to BackTracking
# behavior for the MVP and full upstream line searches remain deferred.

export BackTracking,
    HagerZhang,
    hagerzhang_search,
    LineSearchException,
    MoreThuente,
    StrongWolfe,
    Static,
    InitialPrevious,
    InitialStatic,
    InitialHagerZhang

"""
    BackTracking(; c_1 = 1e-4, rho_hi = 0.5, rho_lo = 0.1, iterations = 1_000)

Armijo backtracking line search parameters.  `c_1` is the sufficient-decrease
constant, `rho_hi` the backtracking contraction factor, and `iterations` the
maximum number of step contractions.  (Upstream names the contraction factors
`ρ_hi`/`ρ_lo`; ASCII names are used here for the SubsetJuliaVM parser.)
"""
struct BackTracking
    c_1::Float64
    rho_hi::Float64
    rho_lo::Float64
    iterations::Int
end
BackTracking(; c_1 = 1e-4, rho_hi = 0.5, rho_lo = 0.1, iterations = 1_000) =
    BackTracking(c_1, rho_hi, rho_lo, iterations)

# HagerZhang (functional — default BFGS line search, Issue #8059). Defined in
# `hagerzhang.jl`, included below.

# Placeholder line searches (deferred — see Issue #7482 / docs/vm/OPTIM.md).
struct MoreThuente end
MoreThuente(; kwargs...) = MoreThuente()

struct StrongWolfe end
StrongWolfe(; kwargs...) = StrongWolfe()

struct Static end
Static(; kwargs...) = Static()

# Initial-step guessers (only the default constant guess is honored by the MVP).
struct InitialPrevious
    alpha::Float64
end
InitialPrevious(; alpha = 1.0) = InitialPrevious(alpha)

struct InitialStatic
    alpha::Float64
end
InitialStatic(; alpha = 1.0) = InitialStatic(alpha)

struct InitialHagerZhang end
InitialHagerZhang(; kwargs...) = InitialHagerZhang()

include("hagerzhang.jl")

end # module LineSearches
