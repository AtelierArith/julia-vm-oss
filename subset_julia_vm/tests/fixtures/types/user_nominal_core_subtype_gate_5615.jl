using Test

abstract type Animal5615 end
abstract type Mammal5615 <: Animal5615 end
abstract type Vehicle5615 end

struct Dog5615 <: Mammal5615 end
struct Cat5615 <: Animal5615 end

@test Dog5615 <: Animal5615
@test Dog5615 <: Mammal5615
@test !(Cat5615 <: Mammal5615)
@test !(Dog5615 <: Vehicle5615)

@test Tuple{Dog5615} <: Tuple{Animal5615}
@test Tuple{Tuple{Dog5615}, Int64} <: Tuple{Tuple{Animal5615}, Real}
@test !(Tuple{Dog5615} <: Tuple{Vehicle5615})
@test !(Tuple{Cat5615} <: Tuple{Mammal5615})

true
