# Test string concatenation in function (Issue 289)

function f()
    s = ""
    s = s * " "
    return s
end

function g()
    s = "hello"
    s = s * " world"
    return s
end

function h()
    s = ""
    for i in 1:3
        s = s * "x"
    end
    return s
end

# Test basic string concatenation and comparison
result1 = f()
r1_len = length(result1)
check1 = r1_len == 1

result2 = g()
r2_len = length(result2)
check2 = r2_len == 11

result3 = h()
r3_len = length(result3)
check3 = r3_len == 3

check1 && check2 && check3
