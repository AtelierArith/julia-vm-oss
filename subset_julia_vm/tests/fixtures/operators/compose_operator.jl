# Compose operator tests

# Basic composition
f(x) = x + 1
g(x) = x * 2

# Create composed function
h = f ∘ g

# Test basic usage: (f ∘ g)(x) = f(g(x)) = (x * 2) + 1
result = h(3)  # g(3) = 6, f(6) = 7
@assert result == 7

# Test nested composition: a ∘ b ∘ c
a(x) = x + 10
b(x) = x * 3
c(x) = x - 5

abc = a ∘ b ∘ c
# (a ∘ b ∘ c)(2) = a(b(c(2))) = a(b(-3)) = a(-9) = 1
nested_result = abc(2)
@assert nested_result == 1

# Test 4-level composition
d(x) = x ^ 2
abcd = a ∘ b ∘ c ∘ d
# (a ∘ b ∘ c ∘ d)(3) = a(b(c(d(3)))) = a(b(c(9))) = a(b(4)) = a(12) = 22
four_level = abcd(3)
@assert four_level == 22

# Return final result for test verification
result + nested_result + four_level  # 7 + 1 + 22 = 30
