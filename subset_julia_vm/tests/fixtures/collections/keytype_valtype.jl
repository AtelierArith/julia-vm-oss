# Test keytype() and valtype() functions
# keytype(dict) -> key type
# valtype(dict) -> value type

using Test

@testset "keytype and valtype - get key/value types of collections" begin

    result = 0

    # Test keytype for Dict (using Dict() constructor)
    dict = Dict()
    dict["a"] = 1
    dict["b"] = 2
    kt = keytype(dict)
    # In SubsetJuliaVM, keytype returns Any (simplified implementation)
    if kt == Any
        result = result + 1
    end

    # Test valtype for Dict
    vt = valtype(dict)
    # In SubsetJuliaVM, valtype returns Any (simplified implementation)
    if vt == Any
        result = result + 1
    end

    # Test keytype for Array (should be Int64)
    arr = [1, 2, 3]
    kt_arr = keytype(arr)
    if kt_arr == Int64
        result = result + 1
    end

    # Test valtype for Array (should match eltype)
    vt_arr = valtype(arr)
    if vt_arr == Int64
        result = result + 1
    end

    complex_arr = zeros(Complex{Float64}, 2)
    if valtype(complex_arr) == Complex{Float64}
        result = result + 1
    end

    # Test keytype for Tuple (should be Int64)
    tup = (1, 2, 3)
    kt_tup = keytype(tup)
    if kt_tup == Int64
        result = result + 1
    end

    @test (result) == 6.0
end

true  # Test passed
