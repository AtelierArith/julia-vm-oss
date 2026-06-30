# Design: Update iOS Coprime π Estimation Sample

## Goal
Update the existing iOS sample `Coprime π Estimation` to use larger input sizes (N=500 and N=1000) as requested, matching the code shown by the user.

## Files to Change
1. `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/coprime_pi_estimation.jl`
2. `SubsetJuliaVMApp/SubsetJuliaVMApp/Models/CodeSamples+Advanced.swift`

## Design

### Julia Sample File
Replace the entire contents of `coprime_pi_estimation.jl` with:

```julia
# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

@time println("N=500: π ≈ ", calc_pi(500))
@time println("N=1000: π ≈ ", calc_pi(1000))
```

Changes from the current version:
- Replace `@time println("N=100: π ≈ ", calc_pi(100))` with `@time println("N=500: π ≈ ", calc_pi(500))`
- Replace `@time println("N=500: π ≈ ", calc_pi(500))` with `@time println("N=1000: π ≈ ", calc_pi(1000))`
- Remove the trailing `println(calc_pi(100))`

### Swift Fallback
Synchronize the matching `code:` string in `CodeSamples+Advanced.swift` so the in-app fallback preview matches the file content exactly.

### Metadata
`SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/samples.json` already contains the correct id, name, category, description, difficulty, tags, and folder. No change needed.

## Verification
- Run the updated `.jl` file with `cargo run --bin sjulia --features repl -- <path>` to confirm it parses and executes.
- Confirm `CodeSamples+Advanced.swift` compiles (the string literal is identical in structure, only line contents change).

## Out of Scope
- No new sample entry in `samples.json`.
- No icon/visual asset changes.
- No build/PR actions beyond editing the two source files.
