# GlobalRef tests
# GlobalRef(mod, name) creates a reference to a global variable in a module

using Test

@testset "GlobalRef constructor and field access: GlobalRef(mod, name), .mod, .name" begin

    # Test GlobalRef constructor with Symbol module name
    gr = GlobalRef(:Main, :x)
    println(typeof(gr))  #> GlobalRef

    # Test GlobalRef field access - .mod and .name
    println(typeof(gr.mod))   #> Module
    println(typeof(gr.name))  #> Symbol

    # Test GlobalRef string representation
    gr2 = GlobalRef(:Base, :println)
    println(gr2)  #> GlobalRef(Base, :println)

    # Verify .name returns the correct symbol
    gr3 = GlobalRef(:Core, :foo)
    result1 = (gr3.name == :foo)

    # Return true if all tests pass
    @test (result1)
end

true  # Test passed
