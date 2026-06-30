# Issue #5057: complete supertype / subtypes reflection for user-defined types.
#
# subtypes(T) returns the direct subtypes of an abstract type, merging the
# builtin lattice with user-defined struct/abstract children. The list is
# deduplicated (Base abstract types appear in both the builtin lattice and the
# runtime registry) and sorted by string name, matching upstream
# `InteractiveUtils.subtypes`. Parametric user structs surface as their base
# `UnionAll` name (`Box`), never as monomorphized instantiations (`Box{Int64}`).
#
# Verified against upstream Julia 1.12 (`julia` + `using InteractiveUtils`).

using Test
using InteractiveUtils

abstract type Vehicle5057 end
abstract type LandVehicle5057 <: Vehicle5057 end

struct Car5057 <: LandVehicle5057
    wheels::Int64
end

mutable struct Truck5057 <: LandVehicle5057
    payload::Int64
end

struct Boat5057 <: Vehicle5057
    length::Int64
end

struct Crate5057{T} <: Vehicle5057
    contents::T
end

struct Unrelated5057
    v::Int64
end

@testset "supertype user + builtin types (Issue #5057)" begin
    # Builtin types.
    @test supertype(Int) === Signed
    @test supertype(Int64) === Signed
    @test supertype(Signed) === Integer
    @test supertype(Integer) === Real
    @test supertype(Real) === Number
    @test supertype(Bool) === Integer
    @test supertype(Float64) === AbstractFloat
    @test supertype(Any) === Any

    # User abstract types.
    @test supertype(Vehicle5057) === Any
    @test supertype(LandVehicle5057) === Vehicle5057

    # User concrete structs.
    @test supertype(Car5057) === LandVehicle5057
    @test supertype(Truck5057) === LandVehicle5057
    @test supertype(Boat5057) === Vehicle5057
    @test supertype(Unrelated5057) === Any
end

@testset "subtypes builtin lattice is sorted and deduplicated (Issue #5057)" begin
    # Sorted by string name, no duplicates, matching upstream subtypes(Integer).
    @test subtypes(Integer) == Any[Bool, Signed, Unsigned]
    @test subtypes(Signed) == Any[BigInt, Int128, Int16, Int32, Int64, Int8]
    @test subtypes(Unsigned) == Any[UInt128, UInt16, UInt32, UInt64, UInt8]
    # A concrete leaf type has no subtypes.
    @test isempty(subtypes(Int64))
    @test isempty(subtypes(Float64))
end

@testset "subtypes user hierarchy (Issue #5057)" begin
    # Force instantiation of the parametric struct so monomorphized defs exist;
    # subtypes must still report the base `Crate5057`, not `Crate5057{Int64}`.
    c1 = Crate5057(1)
    c2 = Crate5057("x")
    @test c1.contents == 1
    @test c2.contents == "x"

    # Sorted by string name: Boat < Car? No — only direct children of Vehicle.
    # Direct children of Vehicle5057: Boat5057, Crate5057, LandVehicle5057.
    veh = subtypes(Vehicle5057)
    @test veh == Any[Boat5057, Crate5057, LandVehicle5057]

    # Direct children of LandVehicle5057: Car5057, Truck5057 (sorted).
    @test subtypes(LandVehicle5057) == Any[Car5057, Truck5057]

    # Parametric instantiation never leaks into the list.
    for t in veh
        @test string(t) != "Crate5057{Int64}"
        @test string(t) != "Crate5057{String}"
    end
end

true
