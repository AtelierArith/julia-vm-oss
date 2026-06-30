# Keyboard Shortcuts

SubsetJuliaVM supports keyboard shortcuts when using an external keyboard with your iPad or iPhone.

## Editor View

### Text Editing

| Shortcut | Action |
|----------|--------|
| `Cmd + Z` | Undo |
| `Cmd + Shift + Z` | Redo |
| `Cmd + A` | Select all |
| `Cmd + C` | Copy |
| `Cmd + V` | Paste |
| `Cmd + X` | Cut |

### Code Execution

| Shortcut | Action |
|----------|--------|
| `Cmd + R` | Run code |
| `Cmd + .` | Stop execution |

### Navigation

| Shortcut | Action |
|----------|--------|
| `Cmd + ↑` | Go to beginning |
| `Cmd + ↓` | Go to end |
| `Option + ←` | Move word left |
| `Option + →` | Move word right |

## REPL View

### Input

| Shortcut | Action |
|----------|--------|
| `Return` | Evaluate input |
| `↑` | Previous history item |
| `↓` | Next history item |
| `Cmd + K` | Clear display |

### Text Editing

| Shortcut | Action |
|----------|--------|
| `Cmd + Z` | Undo in input |
| `Cmd + A` | Select all in input |
| `Ctrl + A` | Go to line start |
| `Ctrl + E` | Go to line end |

## Universal Shortcuts

These work throughout the app:

| Shortcut | Action |
|----------|--------|
| `Cmd + ,` | Settings (if available) |
| `Tab` | Unicode completion (after `\`) |

## Touch Gestures

For on-screen use without an external keyboard:

| Gesture | Action |
|---------|--------|
| **Tap** | Position cursor |
| **Double-tap** | Select word |
| **Triple-tap** | Select line |
| **Pinch** | Zoom text (if supported) |
| **Two-finger tap** | Undo |
| **Three-finger tap** | Redo |

## Tips

### Faster Coding with Keyboard

1. **Use Cmd+R** to run without reaching for the Run button
2. **Arrow keys + history** in REPL for quick iterations
3. **Tab completion** for Unicode saves typing Greek letters

### Multi-Line REPL Input

When entering multi-line code in REPL:
- Press `Return` to continue to next line (when incomplete)
- The REPL auto-detects incomplete input
- Final `Return` after `end` evaluates everything

### Copying Results

1. Select output text in the log
2. Use `Cmd + C` to copy
3. Paste elsewhere with `Cmd + V`
