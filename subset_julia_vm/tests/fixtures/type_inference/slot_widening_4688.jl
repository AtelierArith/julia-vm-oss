using Test

function for_int_overwrite_4688(n)
    x = "init"
    for i in 1:n
        x = 42
    end
    x
end

function for_str_overwrite_4688(n)
    x = 1
    for i in 1:n
        x = "s"
    end
    x
end

function for_float_overwrite_4688(n)
    x = "f"
    for i in 1:n
        x = 1.5
    end
    x
end

function if_mixed_assign_4688(b)
    x = "init"
    if b
        x = 42
    end
    x
end

@testset "slot widening for Union locals across mixed-type assignments (Issue #4688)" begin
    # The for-loop body's `x = 42` previously latched the specializer's
    # slot type tracking on `I64`, even though the pre-loop `x = "init"`
    # bound x to a String. When the loop ran zero iterations the slot
    # still held the surviving String, and the final `LoadSlotI64`
    # crashed with `expected numeric in x, got Str("init")`. The fix
    # widens the recorded slot type to `Any` whenever an assignment
    # rebinds the variable to a different concrete type, so subsequent
    # loads emit `LoadAny` / `LoadSlot` and survive either value at
    # runtime.
    @test for_int_overwrite_4688(3) == 42
    @test for_int_overwrite_4688(0) == "init"
    @test for_int_overwrite_4688(1) == 42

    @test for_str_overwrite_4688(3) == "s"
    @test for_str_overwrite_4688(0) == 1
    @test for_str_overwrite_4688(1) == "s"

    @test for_float_overwrite_4688(3) == 1.5
    @test for_float_overwrite_4688(0) == "f"

    # Reassignment inside an `if` exhibits the same shape (one branch
    # rebinds to a new concrete type, the other preserves the initial
    # binding). Without the widening fix, the `else` (no-rebind) path
    # would mis-load through the body's typed slot.
    @test if_mixed_assign_4688(true) == 42
    @test if_mixed_assign_4688(false) == "init"
end

true
