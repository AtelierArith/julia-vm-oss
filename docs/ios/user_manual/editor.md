# Editor View

The Editor view is designed for writing and running multi-line Julia programs.

## Interface Overview

```
┌─────────────────────────────────────┐
│  [Samples] [Undo] [Redo]   [Run ▶]  │  ← Toolbar
├─────────────────────────────────────┤
│                                     │
│  function hello()                   │
│      println("Hello!")              │  ← Code Editor
│  end                                │
│  hello()                            │
│                                     │
├─────────────────────────────────────┤
│  Hello!                             │
│  [Time] 0.002s                      │  ← Output Log
│                                     │
└─────────────────────────────────────┘
```

## Toolbar Buttons

| Button | Action |
|--------|--------|
| **Samples** (folder icon) | Open sample code browser |
| **Undo** | Undo last edit |
| **Redo** | Redo undone edit |
| **Run** (▶) | Execute the code |
| **Stop** | Cancel running execution (appears while running) |

## Code Editor Features

### Syntax Highlighting

The editor uses **Monokai** theme with colors for:
- **Keywords** (purple): `function`, `end`, `if`, `for`, `while`, etc.
- **Strings** (yellow): `"Hello"`
- **Numbers** (purple): `42`, `3.14`
- **Comments** (gray): `# this is a comment`
- **Operators** (red): `+`, `-`, `*`, `/`, `==`

### Line Numbers

Line numbers appear on the left side. When an error occurs, the problematic line is highlighted.

### Auto-Indentation

The editor automatically indents after:
- `function`, `if`, `for`, `while`, `try`, `begin`, `let`
- Pressing Return after these keywords adds proper indentation

## Output Log

The log panel shows:

| Output Type | Format |
|-------------|--------|
| **println output** | Plain text, real-time |
| **Execution time** | `[Time] 0.123s` |
| **Errors** | `[Error] message` with line info |
| **Cancellation** | `[Stop] Execution cancelled` |

### Real-Time Output

Output from `println` appears immediately, even for long-running programs. You don't have to wait for execution to complete.

### Error Display

Errors show:
- Error message
- Line number (when available)
- Hint for fixing (for some errors)

Example:
```
[Error] Undefined variable: x
Line 3, Column 5
Hint: Did you mean to define 'x' first?
```

## Running Code

### Start Execution

1. Tap **Run** (▶) button
2. Button changes to show "Running..."
3. Output appears in the log panel

### Stop Execution

For long-running code:
1. **Stop** button appears while running
2. Tap **Stop** to cancel
3. Log shows `[Stop] Execution cancelled`

## Sample Browser

Access via the folder icon in toolbar:

### Categories

- **Basic** - Variables, printing, arithmetic
- **Arrays** - Creating and manipulating arrays
- **Loops** - For and while loops
- **Functions** - Defining and calling functions
- **Higher-Order** - map, filter, reduce, lambdas
- **Structures** - Structs and custom types
- **Error Handling** - try/catch/finally
- **Algorithms** - Sorting, searching, etc.
- **Monte Carlo** - Random simulations
- **Mathematics** - Math functions and constants
- **Macros** - @time, @assert, @show

### Difficulty Levels

- **Beginner** - Simple concepts, short code
- **Intermediate** - Combining features
- **Advanced** - Complex algorithms, optimization

## Tips

- **Use Stop for infinite loops** - If you accidentally write an infinite loop
- **Check line numbers on errors** - They point to the exact problem location
- **Save interesting code** - Copy to Notes app before switching samples
- **Use REPL for testing** - Try small expressions before adding to program
