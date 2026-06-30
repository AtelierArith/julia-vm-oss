using Test

struct HasMethodDispatchBox3911
end

function hasmethod_probe_3911(x::Int64)
    x + 1
end

Base.hasmethod(::HasMethodDispatchBox3911, ::Type{Tuple{String}}) = true

@test Base.hasmethod(HasMethodDispatchBox3911(), Tuple{String})
@test Base.hasmethod(hasmethod_probe_3911, Tuple{Int64})
@test !Base.hasmethod(hasmethod_probe_3911, Tuple{Float64})

true
