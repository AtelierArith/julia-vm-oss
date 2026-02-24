# Test multi-level abstract type hierarchy dispatch (Issue #3147)
# Dispatch should correctly route through intermediate abstract type levels.

using Test

# Multi-level hierarchy with sibling abstract types
abstract type Vehicle end
abstract type MotorVehicle <: Vehicle end
abstract type NonMotorVehicle <: Vehicle end
struct Car <: MotorVehicle end
struct Bicycle <: NonMotorVehicle end

f(::MotorVehicle) = "motor"
f(::NonMotorVehicle) = "non-motor"

@test f(Car()) == "motor"
@test f(Bicycle()) == "non-motor"

true
