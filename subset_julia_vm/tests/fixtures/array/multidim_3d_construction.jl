# Test 3D array construction and multi-dimensional indexing

# 3D array via zeros + setindex!
A = zeros(2, 3, 2)
for i in 1:2
    for j in 1:3
        for k in 1:2
            A[i, j, k] = Float64(i * 100 + j * 10 + k)
        end
    end
end

# Verify shape
println(ndims(A) == 3)
println(size(A) == (2, 3, 2))
println(length(A) == 12)

# Verify specific element access
println(A[1, 1, 1] == 111.0)
println(A[2, 1, 1] == 211.0)
println(A[1, 2, 1] == 121.0)
println(A[1, 1, 2] == 112.0)
println(A[2, 3, 2] == 232.0)

# Verify corner elements
println(A[1, 1, 1] == 111.0)
println(A[2, 3, 2] == 232.0)

# Sum all elements via a function
function sum_3d(arr)
    s = 0.0
    for i in 1:2
        for j in 1:3
            for k in 1:2
                s = s + arr[i, j, k]
            end
        end
    end
    return s
end
println(sum_3d(A) == 2058.0)

true
