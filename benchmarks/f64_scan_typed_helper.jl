# Issue #10491 benchmark twin: fully-typed Float64 helper (annotation-only
# difference from f64_scan_untyped_helper.jl).
function fstep(x::Float64, y::Float64)::Float64
    r = x
    k = 0
    while k < 4
        r = r + y
        r = r * 0.5
        k = k + 1
    end
    r
end

function scan(N::Int64)::Int64
    cnt = 0
    x = 0.0
    a = 1
    while a <= N
        x = x + 1.0
        y = 0.0
        b = 1
        while b <= N
            y = y + 1.0
            if fstep(x, y) > 1.5
                cnt = cnt + 1
            end
            b = b + 1
        end
        a = a + 1
    end
    cnt
end

println(scan(2000))
