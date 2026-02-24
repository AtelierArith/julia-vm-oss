# Test mod1, fld1, fldmod1 functions
# These functions implement 1-based modular arithmetic
# mod1(x, y) returns values in (0, y] instead of [0, y)
# fld1(x, y) returns floor division consistent with mod1
# The relationship: x == (fld1(x, y) - 1) * y + mod1(x, y)

result = 0

# ===== mod1 tests =====

# Test 1: mod1(4, 2) = 2 (not 0 like mod)
if mod1(4, 2) == 2
    result = result + 1
end

# Test 2: mod1(3, 3) = 3 (not 0)
if mod1(3, 3) == 3
    result = result + 1
end

# Test 3: mod1(1, 3) = 1
if mod1(1, 3) == 1
    result = result + 1
end

# Test 4: mod1(5, 3) = 2
if mod1(5, 3) == 2
    result = result + 1
end

# Test 5: mod1(6, 3) = 3 (not 0)
if mod1(6, 3) == 3
    result = result + 1
end

# Test 6: mod1 with negative numbers - mod1(-1, 3) = 2
if mod1(-1, 3) == 2
    result = result + 1
end

# ===== fld1 tests =====

# Test 7: fld1(15, 4) = 4
# Because: (4-1)*4 + mod1(15,4) = 3*4 + 3 = 15
if fld1(15, 4) == 4
    result = result + 1
end

# Test 8: fld1(4, 2) = 2
# Because: (2-1)*2 + mod1(4,2) = 1*2 + 2 = 4
if fld1(4, 2) == 2
    result = result + 1
end

# Test 9: fld1(6, 3) = 2
# Because: (2-1)*3 + mod1(6,3) = 1*3 + 3 = 6
if fld1(6, 3) == 2
    result = result + 1
end

# Test 10: fld1(7, 3) = 3
# Because: (3-1)*3 + mod1(7,3) = 2*3 + 1 = 7
if fld1(7, 3) == 3
    result = result + 1
end

# ===== fldmod1 tests =====

# Test 11: fldmod1(15, 4) = (4, 3)
(d, m) = fldmod1(15, 4)
if d == 4 && m == 3
    result = result + 1
end

# Test 12: fldmod1(6, 3) = (2, 3)
(d2, m2) = fldmod1(6, 3)
if d2 == 2 && m2 == 3
    result = result + 1
end

# ===== Verify relationship: x == (fld1(x,y) - 1) * y + mod1(x,y) =====

# Test 13: Verify relationship for x=15, y=4
x = 15
y = 4
if x == (fld1(x, y) - 1) * y + mod1(x, y)
    result = result + 1
end

# Test 14: Verify relationship for x=7, y=3
x2 = 7
y2 = 3
if x2 == (fld1(x2, y2) - 1) * y2 + mod1(x2, y2)
    result = result + 1
end

result  # Expected: 14
