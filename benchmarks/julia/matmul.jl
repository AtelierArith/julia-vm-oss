# Matrix multiplication benchmark
# Tests nested loops and 2D array access

function matmul(A, B, n)
    # Matrices stored as 1D arrays in column-major order
    C = Float64[]
    for i in 1:(n * n)
        push!(C, 0.0)
    end

    for i in 1:n
        for j in 1:n
            s = 0.0
            for k in 1:n
                # A[i,k] = A[(k-1)*n + i], B[k,j] = B[(j-1)*n + k]
                a_idx = (k - 1) * n + i
                b_idx = (j - 1) * n + k
                s = s + A[a_idx] * B[b_idx]
            end
            # C[i,j] = C[(j-1)*n + i]
            c_idx = (j - 1) * n + i
            C[c_idx] = s
        end
    end
    C
end

# Benchmark entry point
function main()
    n = 50

    # Initialize matrices
    A = Float64[]
    B = Float64[]
    for i in 1:(n * n)
        push!(A, Float64(i))
        push!(B, Float64(i))
    end

    C = matmul(A, B, n)

    # Print checksum (sum of all elements)
    s = 0.0
    for i in 1:(n * n)
        s = s + C[i]
    end
    println(s)
end

main()
