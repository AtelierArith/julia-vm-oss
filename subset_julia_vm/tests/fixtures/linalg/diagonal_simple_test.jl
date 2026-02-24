# Simple test for Diagonal type
using LinearAlgebra

# Create a Diagonal matrix
D = Diagonal([1.0, 2.0, 3.0])

# Test size
s1 = size(D, 1)
s2 = size(D, 2)

# Test indexing
v1 = D[1, 1]
v2 = D[1, 2]

s1 + s2 + v1 + v2
