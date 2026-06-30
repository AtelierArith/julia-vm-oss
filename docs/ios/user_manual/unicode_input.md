# Unicode Input

SubsetJuliaVM supports Unicode characters in your code, just like standard Julia. You can use Greek letters, mathematical symbols, and other Unicode characters as variable names and operators.

## LaTeX-Style Input

Type LaTeX commands followed by **Tab** to convert them to Unicode symbols.

### How to Use

1. Type a backslash `\` followed by the LaTeX name
2. Press **Tab** to convert
3. The LaTeX command transforms into the Unicode character

Example:
```
\alpha  →  α
\beta   →  β
\pi     →  π
```

## Common Symbols

### Greek Letters

| Input | Symbol | Name |
|-------|--------|------|
| `\alpha` | α | alpha |
| `\beta` | β | beta |
| `\gamma` | γ | gamma |
| `\delta` | δ | delta |
| `\epsilon` | ε | epsilon |
| `\theta` | θ | theta |
| `\lambda` | λ | lambda |
| `\mu` | μ | mu |
| `\pi` | π | pi |
| `\sigma` | σ | sigma |
| `\phi` | φ | phi |
| `\omega` | ω | omega |

Capital letters: `\Alpha`, `\Beta`, `\Gamma`, etc.

### Mathematical Operators

| Input | Symbol | Usage |
|-------|--------|-------|
| `\times` | × | multiplication |
| `\div` | ÷ | division |
| `\pm` | ± | plus-minus |
| `\cdot` | · | dot product |
| `\le` | ≤ | less or equal |
| `\ge` | ≥ | greater or equal |
| `\ne` | ≠ | not equal |
| `\approx` | ≈ | approximately |
| `\in` | ∈ | element of |
| `\notin` | ∉ | not element of |
| `\subset` | ⊂ | subset |
| `\cup` | ∪ | union |
| `\cap` | ∩ | intersection |

### Arrows

| Input | Symbol |
|-------|--------|
| `\to` | → |
| `\rightarrow` | → |
| `\leftarrow` | ← |
| `\Rightarrow` | ⇒ |
| `\Leftarrow` | ⇐ |
| `\leftrightarrow` | ↔ |

### Subscripts and Superscripts

| Input | Symbol | Example |
|-------|--------|---------|
| `\_0` - `\_9` | ₀-₉ | x₁, x₂ |
| `\^0` - `\^9` | ⁰-⁹ | x², x³ |
| `\_a` - `\_z` | ₐ-ₓ | xₙ |

## Using Unicode in Code

### Variable Names

Unicode characters work as valid identifiers:

```julia
α = 0.5
β = 0.3
γ = 1 - α - β

θ = π / 4
r = cos(θ)
```

### Mathematical Expressions

Makes code look like mathematical notation:

```julia
# Quadratic formula
Δ = b^2 - 4a*c
x₁ = (-b + √Δ) / 2a
x₂ = (-b - √Δ) / 2a
```

### Physics and Science

```julia
# Kinetic energy
m = 10.0  # mass in kg
v = 5.0   # velocity in m/s
E = ½ * m * v²

# Einstein's equation
c = 299792458  # speed of light
E = m * c²
```

## Built-in Constants

Some Unicode constants are predefined:

| Symbol | Value | LaTeX |
|--------|-------|-------|
| `π` | 3.14159... | `\pi` |
| `ℯ` | 2.71828... | `\euler` |

```julia
circumference = 2π * radius
growth = ℯ^rate
```

## Tips

### Finding Symbols

If you don't remember the LaTeX name:
- Try the obvious name: `\alpha`, `\infinity`, `\sum`
- Check Julia documentation for the standard list
- Most common mathematical symbols are supported

### Readability

Unicode makes code more readable for mathematical work:

```julia
# Without Unicode
function quadratic(a, b, c)
    discriminant = b^2 - 4*a*c
    x1 = (-b + sqrt(discriminant)) / (2*a)
    x2 = (-b - sqrt(discriminant)) / (2*a)
    return x1, x2
end

# With Unicode
function quadratic(a, b, c)
    Δ = b² - 4a*c
    x₁ = (-b + √Δ) / 2a
    x₂ = (-b - √Δ) / 2a
    return x₁, x₂
end
```

### When to Use Unicode

**Good for:**
- Mathematical formulas
- Scientific notation
- Making code match paper/textbook notation

**Avoid when:**
- Code will be shared with non-Julia users
- Typing on systems without Unicode support
- Variable names need to be spoken aloud

## Troubleshooting

### Tab Completion Not Working

- Make sure you typed the backslash `\` first
- Check spelling of the LaTeX command
- Not all LaTeX commands are supported

### Symbol Not Recognized

If a symbol isn't converting:
- Try an alternative spelling
- Use the ASCII equivalent instead
- Check if it's a supported symbol
