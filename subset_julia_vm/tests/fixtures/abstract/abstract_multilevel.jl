using Test

# Multi-level abstract type hierarchy with dispatch on sibling abstract types (Issue #3144)
abstract type Vehicle end
abstract type MotorVehicle <: Vehicle end
abstract type NonMotorVehicle <: Vehicle end

struct Car <: MotorVehicle
    speed::Int
end

struct Bicycle <: NonMotorVehicle
    gears::Int
end

# Dispatch on the abstract type level (sibling abstracts — tests Issue #3144 fix)
function vehicle_type(v::MotorVehicle)
    "motor"
end

function vehicle_type(v::NonMotorVehicle)
    "non-motor"
end

@testset "multi-level abstract type hierarchy" begin
    car = Car(120)
    bike = Bicycle(21)

    @test vehicle_type(car) == "motor"
    @test vehicle_type(bike) == "non-motor"

    # Subtype checks
    @test Car <: MotorVehicle
    @test Car <: Vehicle
    @test Bicycle <: NonMotorVehicle
    @test Bicycle <: Vehicle

    # Not subtype
    @test !(Car <: NonMotorVehicle)
    @test !(Bicycle <: MotorVehicle)

    # Struct field access
    @test car.speed == 120
    @test bike.gears == 21
end

true
