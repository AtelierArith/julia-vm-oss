using Test

# Outside a struct constructor, explicit type parameters apply to the ordinary
# global binding named `new`; they must not be discarded by the ownerless
# fallback (#11204).
new = Vector
function parametric_ownerless_new11204()
    new{Int}(undef, 2)
end

ownerless_parametric11204 = parametric_ownerless_new11204()
@test typeof(ownerless_parametric11204) === Vector{Int}
@test length(ownerless_parametric11204) == 2

true
