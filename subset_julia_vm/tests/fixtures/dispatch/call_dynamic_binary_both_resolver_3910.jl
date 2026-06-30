using Test

struct BinaryBothBox3910
    value::Int64
end

Base.:+(box::BinaryBothBox3910, x::Any) = box.value + 1000
Base.:+(box::BinaryBothBox3910, x::Float64) = box.value + 100
Base.:+(box::BinaryBothBox3910, x::Int64) = box.value + x

function binary_both_any_dispatch_3910(box::Any, x::Any)
    return box + x
end

@testset "CallDynamicBinaryBoth shared resolver (Issue #3910)" begin
    box = BinaryBothBox3910(10)

    @test binary_both_any_dispatch_3910(box, 5) == 15
    @test binary_both_any_dispatch_3910(box, 2.5) == 110
    @test binary_both_any_dispatch_3910(box, "fallback") == 1010
end

true
