# Issue #7020: a 3-arg (step) range `a:b:c` is parsed nested as `(a:b):c`. When a
# macro `esc`s such a range, the quote→code path must flatten it back to a step
# range; otherwise re-lowering builds `Range : c` and the VM fails at runtime with
# "expected numeric value, got Range". (Surfaced by @animate/@gif, Issue #6355.)

macro passthru(e)
    quote
        $(esc(e))
    end
end

# Float step range through esc must iterate element-wise (not bind x to the Range).
acc = Float64[]
@passthru for x = 0:0.1:0.5
    push!(acc, x)
end

# 0:0.1:0.5 has 6 elements.
ok_count = length(acc) == 6
ok_first = acc[1] == 0.0
ok_last = acc[6] == 0.5

# Integer step range too.
r = @passthru(1:2:9)
ok_step = collect(r) == [1, 3, 5, 7, 9]

ok_count && ok_first && ok_last && ok_step
