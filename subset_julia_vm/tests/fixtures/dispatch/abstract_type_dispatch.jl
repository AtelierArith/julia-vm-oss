# Test method dispatch with abstract type parameters (Issue #636)
# Methods with abstract type parameters should work regardless of definition order

using Test

# Test case 1: Method defined AFTER struct types
abstract type AbstractIrrational <: Real end
struct IrrationalPi <: AbstractIrrational end
struct IrrationalE <: AbstractIrrational end

# Method using abstract type parameter - defined after struct
to_float(x::AbstractIrrational) = 3.14
@test to_float(IrrationalPi()) == 3.14
@test to_float(IrrationalE()) == 3.14

# Test case 2: Multiple methods with different abstract types
abstract type Animal end
abstract type Pet <: Animal end
struct Dog <: Pet end
struct Cat <: Pet end

speak(::Animal) = "?"
speak(::Pet) = "hello"
speak(::Dog) = "woof"
speak(::Cat) = "meow"

# Most specific method should be selected
@test speak(Dog()) == "woof"
@test speak(Cat()) == "meow"

# Test case 3: Hierarchy with Real
abstract type MyNumber <: Real end
struct MyInt <: MyNumber end

double(x::MyNumber) = 2
@test double(MyInt()) == 2

# Return true to indicate success
true
