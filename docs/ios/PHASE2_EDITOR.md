# Phase 2: Code Editor Enhancements

**Goal**: シンタックスハイライト、行番号、UX改善

**Status**: ⏳ **部分完了**

## 完了済み

- ✅ Monokai テーマでのシンタックスハイライト
- ✅ 日本語 IME 入力対応
- ✅ 出力テキスト選択（コピー可能）

## Task 2.1: Syntax Highlighting ✅

**File**: `Views/Editor/SyntaxHighlighter.swift`

```swift
struct JuliaSyntaxHighlighter {
    private static let keywords: Set<String> = [
        "function", "end", "if", "else", "elseif",
        "for", "while", "break", "continue", "return",
        "true", "false", "in"
    ]

    private static let builtins: Set<String> = [
        "println", "print", "rand", "sqrt", "ifelse"
    ]

    static func highlight(_ code: String) -> AttributedString {
        // Tokenize and apply colors
    }
}
```

**Checklist**:
- [x] Implement syntax highlighter ✅
- [x] Test with various Julia code samples ✅

## Task 2.2: Line Numbers ⏳

**File**: `Views/Editor/LineNumberView.swift`

```swift
struct LineNumberView: View {
    let lineCount: Int
    let font: Font
    let highlightedLine: Int?

    var body: some View {
        VStack(alignment: .trailing, spacing: 0) {
            ForEach(1...lineCount, id: \.self) { line in
                Text("\(line)")
                    .font(font)
                    .foregroundColor(line == highlightedLine ? .blue : .gray)
                    .frame(width: 30, alignment: .trailing)
                    .background(
                        line == highlightedLine
                        ? Color.blue.opacity(0.1)
                        : Color.clear
                    )
            }
        }
    }
}
```

**File**: `Views/Editor/CodeEditorView.swift`

```swift
struct CodeEditorView: View {
    @Binding var text: String
    let error: VMError?

    var body: some View {
        HStack(spacing: 0) {
            // Line numbers (synced scroll)
            LineNumberView(...)

            // Editor
            TextEditor(text: $text)
        }
    }
}
```

**Checklist**:
- [ ] Implement line number view
- [ ] Sync scrolling with editor
- [ ] Highlight error line
- [ ] Make font size configurable
- [ ] Test with long files

## Task 2.3: Editor Features ⏳

### Auto-indentation

```swift
extension String {
    func autoIndent(at index: String.Index) -> String {
        // Detect indentation of current line
        // Insert matching indentation on newline
    }
}
```

### Tab to Spaces

```swift
extension String {
    func replacingTabs(with spaces: Int = 4) -> String {
        replacingOccurrences(of: "\t", with: String(repeating: " ", count: spaces))
    }
}
```

**Checklist**:
- [ ] Tab to spaces conversion
- [ ] Basic auto-indentation (optional)
- [ ] Bracket matching (optional)
- [ ] Cursor position tracking

## Acceptance Criteria

- [x] Syntax highlighting works for keywords, strings, numbers, comments ✅
- [ ] Line numbers displayed correctly
- [ ] Line numbers scroll with code
- [ ] Error line highlighted in line numbers
- [x] Editor responsive with 500+ line files ✅
- [ ] Tab key inserts spaces
