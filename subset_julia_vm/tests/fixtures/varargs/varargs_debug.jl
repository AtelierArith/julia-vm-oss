# Debug varargs

using Test

function inspect(args...)
    println("Inside inspect:")
    println("  length(args) = ", length(args))
    if length(args) > 0
        println("  args[1] = ", args[1])
        println("  typeof(args[1]) = ", typeof(args[1]))
    end
    if length(args) > 1
        println("  args[2] = ", args[2])
    end
    if length(args) > 2
        println("  args[3] = ", args[3])
    end
    true
end

@testset "Varargs with inspection" begin


    @test (inspect(100, 200, 300))
end

true  # Test passed
