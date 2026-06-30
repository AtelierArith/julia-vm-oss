# Sample Codes

SubsetJuliaVM includes 38 sample programs organized by category and difficulty level. These examples demonstrate Julia features and serve as learning resources.

## Accessing Samples

1. In Editor view, tap the **folder icon** in the toolbar
2. Browse samples by category
3. Tap a sample to load it
4. Tap **Run** to execute

## Categories

### Basic

Fundamental Julia concepts for beginners.

| Sample | Description |
|--------|-------------|
| Hello World | Print a greeting |
| Variables | Assign and use variables |
| Arithmetic | Basic math operations |
| String Interpolation | Embed values in strings |
| Comments | Single and block comments |

Example - String Interpolation:
```julia
name = "Julia"
version = 1.0
println("Welcome to $name $(version)!")
# Output: Welcome to Julia 1.0!
```

### Arrays

Creating and manipulating arrays.

| Sample | Description |
|--------|-------------|
| Vector Basics | Create and access 1D arrays |
| Matrix Operations | 2D arrays and indexing |
| Array Comprehensions | Create arrays with expressions |
| Broadcasting | Element-wise operations |
| Push and Pop | Dynamic array modification |

Example - Array Comprehension:
```julia
squares = [x^2 for x in 1:5]
# Result: [1, 4, 9, 16, 25]

evens = [x for x in 1:10 if x % 2 == 0]
# Result: [2, 4, 6, 8, 10]
```

### Loops

Iteration patterns.

| Sample | Description |
|--------|-------------|
| For Loop | Iterate over ranges |
| While Loop | Condition-based iteration |
| Nested Loops | Loops within loops |
| Break and Continue | Control flow in loops |
| Range Expressions | Different range syntaxes |

Example - For Loop with Range:
```julia
for i in 1:5
    println("i = $i")
end

for i in 10:-2:0  # step by -2
    println(i)
end
```

### Functions

Defining and using functions.

| Sample | Description |
|--------|-------------|
| Simple Function | Basic function definition |
| Multiple Arguments | Functions with many parameters |
| Return Values | Explicit and implicit returns |
| Recursion | Functions that call themselves |
| Multiple Dispatch | Different methods for types |

Example - Recursion:
```julia
function factorial(n)
    if n <= 1
        return 1
    else
        return n * factorial(n - 1)
    end
end

println(factorial(5))  # 120
```

### Higher-Order

Functions as values.

| Sample | Description |
|--------|-------------|
| Lambda Expressions | Anonymous functions |
| Map Function | Transform each element |
| Filter Function | Select matching elements |
| Reduce Function | Combine all elements |
| Do Syntax | Block syntax for callbacks |

Example - Map and Filter:
```julia
numbers = [1, 2, 3, 4, 5]

doubled = map(x -> x * 2, numbers)
# Result: [2, 4, 6, 8, 10]

evens = filter(x -> x % 2 == 0, numbers)
# Result: [2, 4]
```

### Structures

Custom data types.

| Sample | Description |
|--------|-------------|
| Immutable Struct | Read-only structures |
| Mutable Struct | Modifiable structures |
| Struct with Functions | Methods on types |
| Parametric Types | Generic structures |

Example - Mutable Struct:
```julia
mutable struct Counter
    value::Int64
end

c = Counter(0)
c.value += 1
println(c.value)  # 1
```

### Error Handling

Managing errors gracefully.

| Sample | Description |
|--------|-------------|
| Try-Catch Basics | Catching errors |
| Finally Block | Cleanup code |
| Error Messages | Working with error info |

Example - Try-Catch:
```julia
function safe_divide(a, b)
    try
        return a / b
    catch
        return "Cannot divide by zero"
    end
end
```

### Algorithms

Classic programming problems.

| Sample | Description |
|--------|-------------|
| Sieve of Eratosthenes | Find prime numbers |
| Bubble Sort | Simple sorting |
| Binary Search | Efficient searching |
| Fibonacci | Classic sequence |
| Newton's Method | Root finding |

### Monte Carlo

Random simulations.

| Sample | Description |
|--------|-------------|
| Estimate Pi | Pi via random points |
| Random Walk | Simulated movement |
| Buffon's Needle | Classic probability |
| Monte Carlo Integration | Numerical integration |

Example - Estimate Pi:
```julia
function estimate_pi(n)
    inside = 0
    for _ in 1:n
        x, y = rand(), rand()
        if x^2 + y^2 <= 1
            inside += 1
        end
    end
    return 4 * inside / n
end

println(estimate_pi(10000))  # ≈ 3.14
```

### Mathematics

Mathematical functions and constants.

| Sample | Description |
|--------|-------------|
| Trigonometry | sin, cos, tan |
| Complex Numbers | Complex arithmetic |
| Taylor Series | Series expansions |
| Statistical Functions | mean, std, etc. |

### Macros

Special compile-time features.

| Sample | Description |
|--------|-------------|
| @time | Measure execution time |
| @assert | Runtime assertions |
| @show | Debug printing |

Example - @time:
```julia
@time begin
    sum = 0
    for i in 1:1000000
        sum += i
    end
    println("Sum: $sum")
end
# Output includes execution time
```

## Difficulty Levels

### Beginner
- Simple, self-contained examples
- 5-15 lines of code
- Single concept demonstration

### Intermediate
- Combines multiple features
- 15-40 lines of code
- More complex logic

### Advanced
- Complete algorithms
- 40+ lines of code
- Performance considerations

## Tips for Learning

1. **Start with Basic** - Build foundation first
2. **Run before reading** - See what it does, then study how
3. **Modify samples** - Change values, add features
4. **Use REPL for parts** - Test pieces of larger samples
5. **Progress gradually** - Master Beginner before Intermediate
