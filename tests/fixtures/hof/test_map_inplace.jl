# Test map! function (in-place map)
# map!(f, dest, src) applies f to each element of src and stores in dest

# Basic map!
src = [1.0, 2.0, 3.0]
dest = [0.0, 0.0, 0.0]
result1 = map!(x -> x * 2, dest, src)
println(dest[1])  # 2.0
println(dest[2])  # 4.0
println(dest[3])  # 6.0

# map! with squares
src2 = [2.0, 3.0, 4.0]
dest2 = [0.0, 0.0, 0.0]
map!(x -> x^2, dest2, src2)
println(dest2[1])  # 4.0
println(dest2[2])  # 9.0
println(dest2[3])  # 16.0

# Verify returned value is dest
arr1 = [1.0, 2.0]
arr2 = [0.0, 0.0]
ret = map!(x -> x + 10, arr2, arr1)
println(ret === arr2)  # Should be true (same reference)
