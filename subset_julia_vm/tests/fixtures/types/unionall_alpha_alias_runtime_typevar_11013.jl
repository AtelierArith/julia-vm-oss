using Test

# Issue #11013: a runtime TypeVar's spelling is diagnostic only. Generic alias
# recognition must bind by identity, including hygiene/gensym names containing
# `#`, and then agree across display, equality, and both subtype directions.
for name in (:Txyz, Symbol("T##m#123_0"))
    var = TypeVar(name)
    wrapper = UnionAll(var, Vector{var})

    @test string(wrapper) == "Vector"
    @test wrapper == Vector
    @test Vector == wrapper
    @test wrapper <: Vector
    @test Vector <: wrapper
end

# Lower bounds make the wrapper a proper subset, not the fully generic alias.
# Alpha projection must preserve that distinction across every semantic lane.
lower_bounded_var = TypeVar(:X, Int, Any)
lower_bounded = UnionAll(lower_bounded_var, Vector{lower_bounded_var})
@test string(lower_bounded) == "Vector{X} where X>:Int64"
@test lower_bounded != Vector
@test Vector != lower_bounded
@test lower_bounded <: Vector
@test !(Vector <: lower_bounded)

true
