# Getting Started

## First Launch

When you first open SubsetJuliaVM, you'll see the **Editor View** with a sample program already loaded. The app is ready to use immediately - no setup required.

## App Layout

The app has two main views, accessible via tabs at the bottom:

| Tab | Description |
|-----|-------------|
| **Editor** | Write multi-line programs, run samples |
| **REPL** | Interactive line-by-line execution |

## Running Your First Program

1. The editor shows a default "Hello World" program
2. Tap the **Run** button (▶) in the toolbar
3. See the output in the **Log** panel below the editor

```julia
println("Hello, Julia!")
```

Output:
```
Hello, Julia!
[Time] 0.001s
```

## Choosing a Sample

1. Tap the **folder icon** in the toolbar to open Sample Browser
2. Browse by category (Basic, Arrays, Functions, etc.)
3. Tap a sample to load it into the editor
4. Tap **Run** to execute

## Switching to REPL

1. Tap the **REPL** tab at the bottom
2. Type Julia expressions one at a time
3. Press **Return** or tap **Run** to evaluate
4. Results appear immediately below your input

```
julia> 1 + 1
2

julia> x = 42
42

julia> x * 2
84
```

## Tips for Beginners

- **Start with Basic samples** - They demonstrate fundamental concepts
- **Use REPL for experimentation** - Quick feedback for trying ideas
- **Use Editor for programs** - Better for multi-line code with functions
- **Check the output panel** - Errors show helpful messages with line numbers

## Next Steps

- [Editor View](editor.md) - Learn all editor features
- [REPL View](repl.md) - Master interactive coding
- [Sample Codes](samples.md) - Explore all 50+ examples
