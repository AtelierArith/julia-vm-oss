# REPL View

The REPL (Read-Eval-Print Loop) provides an interactive Julia session where you can execute code line by line.

## Interface Overview

```
┌─────────────────────────────────────┐
│  [History] [Clear]     [Reset]      │  ← Toolbar
├─────────────────────────────────────┤
│  julia> x = 10                      │
│  10                                 │
│                                     │
│  julia> x + 5                       │
│  15                                 │
│                                     │  ← Session History
│  julia> println("Hello")            │
│  Hello                              │
│                                     │
├─────────────────────────────────────┤
│  julia> [type here...]    [Run]     │  ← Input Field
└─────────────────────────────────────┘
```

## Basic Usage

### Evaluating Expressions

1. Type a Julia expression in the input field
2. Press **Return** or tap **Run**
3. Result appears immediately

```
julia> 2 + 3
5

julia> sqrt(16)
4.0

julia> "Hello" * " World"
"Hello World"
```

### Variables Persist

Variables you define remain available for the entire session:

```
julia> name = "Julia"
"Julia"

julia> println("Hello, $name!")
Hello, Julia!

julia> length(name)
5
```

### Defining Functions

Functions are available after definition:

```
julia> function greet(name)
           println("Hello, $name!")
       end

julia> greet("World")
Hello, World!

julia> greet("Julia")
Hello, Julia!
```

## Multi-Line Input

### Automatic Detection

The REPL automatically detects when input is incomplete:

```
julia> function factorial(n)
           # cursor moves to next line automatically
           if n <= 1
               return 1
           else
               return n * factorial(n - 1)
           end
       end

julia> factorial(5)
120
```

### Multi-Line Triggers

The REPL waits for more input when it sees:
- `function` without matching `end`
- `if`, `for`, `while` without matching `end`
- `begin`, `let` without matching `end`
- `try` without matching `end`
- Unclosed parentheses `(`, brackets `[`, or braces `{`

## Toolbar Actions

| Button | Action |
|--------|--------|
| **History** | Search previous inputs |
| **Clear** | Clear session display (keeps state) |
| **Reset** | Reset session completely (clears all variables) |
| **Stop** | Cancel running execution |

### History Search

1. Tap **History** button
2. Search through previous inputs
3. Tap an entry to insert it

### Clear vs Reset

| Action | Display | Variables | Functions |
|--------|---------|-----------|-----------|
| **Clear** | Cleared | Kept | Kept |
| **Reset** | Cleared | Cleared | Cleared |

## Input Features

### Suppressing Output with `;`

Add semicolon at the end to suppress the result display:

```
julia> x = 100;

julia> y = 200;

julia> x + y
300
```

### Multiple Expressions

Separate multiple expressions with semicolons:

```
julia> a = 1; b = 2; a + b
3
```

## Output Types

### Values

Regular values are displayed with syntax highlighting:

```
julia> [1, 2, 3]
[1, 2, 3]

julia> 3.14159
3.14159

julia> "text"
"text"
```

### Print Output

`println` and `print` output appears before the result:

```
julia> println("Hello"); 42
Hello
42
```

### Errors

Errors are displayed in red with helpful messages:

```
julia> 1 / 0
[Error] Division by zero

julia> undefined_var
[Error] Undefined variable: undefined_var
```

## Tips

- **Use REPL for quick tests** - Try expressions before adding to full programs
- **Variables persist** - Build up state incrementally
- **Reset when stuck** - If things get confusing, tap Reset
- **History is your friend** - Quickly recall and modify previous inputs
- **Semicolon for clean output** - Use `;` when you don't need to see intermediate values

## Keyboard Tips (with external keyboard)

- **Return** - Evaluate current input
- **Up Arrow** - Previous history item
- **Down Arrow** - Next history item
- **Cmd+K** - Clear display
