using Test

# Issue #4854: a `setfield!`/`setproperty!` call form must invalidate the same
# MustAlias-style field-path inference refinement that the surface `x.f = ...`
# (Stmt::FieldAssign) form invalidates. Otherwise a guard like
# `x.value isa Int64` leaves a stale `x.value :: Int64` refinement in scope
# even after the builtin write clears it.

mutable struct Box4854
    value::Union{Int64,Nothing}
end

mutable struct Pair4854
    a::Union{Int64,Nothing}
    b::Union{Int64,Nothing}
end

function setfield_after_guard_4854(x::Box4854)
    if x.value isa Int64
        setfield!(x, :value, nothing)
        return x.value
    end
    return 0
end

function setproperty_after_guard_4854(x::Box4854)
    if x.value isa Int64
        setproperty!(x, :value, nothing)
        return x.value
    end
    return 0
end

function setfield_in_assign_4854(x::Box4854)
    if x.value isa Int64
        y = setfield!(x, :value, nothing)
        return x.value
    end
    return 0
end

function setfield_dynamic_field_4854(x::Pair4854, f::Symbol)
    if x.a isa Int64
        setfield!(x, f, nothing)
        return x.a
    end
    return 0
end

@testset "setfield! invalidates guarded field-path refinement (4854)" begin
    @test Base.infer_return_type(setfield_after_guard_4854, Tuple{Box4854}) == Union{Nothing,Int64}
    @test Core.Compiler.return_type(setfield_after_guard_4854, Tuple{Box4854}) == Union{Nothing,Int64}
    @test Base.infer_return_type(setproperty_after_guard_4854, Tuple{Box4854}) == Union{Nothing,Int64}
    @test Base.infer_return_type(setfield_in_assign_4854, Tuple{Box4854}) == Union{Nothing,Int64}
    @test Base.infer_return_type(setfield_dynamic_field_4854, Tuple{Pair4854,Symbol}) == Union{Nothing,Int64}
end

@testset "runtime values (4854)" begin
    @test setfield_after_guard_4854(Box4854(7)) === nothing
    @test setfield_after_guard_4854(Box4854(nothing)) == 0
end

true
