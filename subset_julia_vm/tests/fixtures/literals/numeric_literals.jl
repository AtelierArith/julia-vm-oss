# Numeric literal tests

# =============================================================================
# Hexadecimal integer literals
# =============================================================================
@assert 0xff == 255
@assert 0xFF == 255
@assert 0x10 == 16
@assert 0xAB == 171
@assert 0xab == 171
@assert 0x1a2b == 6699

# With underscores
@assert 0xff_ff == 65535
@assert 0x1_0000 == 65536

# =============================================================================
# Binary integer literals
# =============================================================================
@assert 0b0 == 0
@assert 0b1 == 1
@assert 0b10 == 2
@assert 0b1010 == 10
@assert 0b11111111 == 255

# With underscores
@assert 0b1111_0000 == 240
@assert 0b1010_1010 == 170

# =============================================================================
# Octal integer literals
# =============================================================================
@assert 0o0 == 0
@assert 0o7 == 7
@assert 0o10 == 8
@assert 0o17 == 15
@assert 0o77 == 63
@assert 0o777 == 511

# With underscores
@assert 0o7_77 == 511

# =============================================================================
# Float32 literals
# =============================================================================
@assert typeof(1.0f0) == Float32
@assert typeof(1f0) == Float32
@assert typeof(1.5f0) == Float32
@assert typeof(1.0f-1) == Float32
@assert typeof(1.0f+1) == Float32

# Value checks
@assert 1.0f0 == Float32(1.0)
@assert 2.5f0 == Float32(2.5)
@assert 1f0 == Float32(1.0)
@assert 1f1 == Float32(10.0)
@assert 1f2 == Float32(100.0)
@assert 1.5f-1 == Float32(0.15)

# =============================================================================
# Hex float literals (with p/P exponent)
# =============================================================================
@assert 0x1p0 == 1.0
@assert 0x1p1 == 2.0
@assert 0x1p2 == 4.0
@assert 0x1p3 == 8.0
@assert 0x1p-1 == 0.5
@assert 0x1.8p0 == 1.5
@assert 0x1.8p3 == 12.0

println("All numeric literal tests passed!")

true  # Test passed
