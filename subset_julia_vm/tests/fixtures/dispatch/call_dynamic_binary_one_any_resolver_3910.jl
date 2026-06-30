using Test

struct BinaryOneAnyBox3910
    value::Int64
end

Base.:+(box::BinaryOneAnyBox3910, x::Any) = box.value + 1000
Base.:+(box::BinaryOneAnyBox3910, x::Real) = box.value + 100
Base.:+(box::BinaryOneAnyBox3910, x::Int64) = box.value + x

function binary_one_any_dispatch_3910(box::BinaryOneAnyBox3910, x::Any)
    box + x
end

box = BinaryOneAnyBox3910(10)

@test binary_one_any_dispatch_3910(box, 5) == 15
@test binary_one_any_dispatch_3910(box, 2.5) == 110
@test binary_one_any_dispatch_3910(box, "fallback") == 1010

true
