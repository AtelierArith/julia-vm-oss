# Phase 6: Advanced Features (Optional)

**Goal**: パワーユーザー向け高度な機能

**Status**: ❌ **未着手（オプション）**

## Task 6.1: Execution History

過去の実行を追跡:

```swift
struct ExecutionHistory: Codable {
    var entries: [ExecutionHistoryEntry]
    let maxEntries = 50

    mutating func add(_ result: ExecutionResult, code: String) {
        let entry = ExecutionHistoryEntry(
            timestamp: result.timestamp,
            code: code,
            result: result
        )
        entries.insert(entry, at: 0)
        if entries.count > maxEntries {
            entries.removeLast()
        }
    }
}

struct ExecutionHistoryEntry: Identifiable, Codable {
    let id = UUID()
    let timestamp: Date
    let code: String
    let result: ExecutionResult
}
```

**Checklist**:
- [ ] Implement execution history model
- [ ] Create history view
- [ ] Test persistence
- [ ] Add clear history option

## Task 6.2: Export/Share

```swift
struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}

// Usage
Button("Share Code") {
    showShareSheet = true
}
.sheet(isPresented: $showShareSheet) {
    ShareSheet(items: [code])
}
```

**Checklist**:
- [ ] Implement share sheet
- [ ] Export as .jl file
- [ ] Export as text
- [ ] Test sharing to different apps

## Task 6.3: Code Templates

スニペットライブラリ:

```swift
struct CodeTemplate: Identifiable {
    let id = UUID()
    let name: String
    let template: String

    static let templates = [
        CodeTemplate(name: "Function", template: """
        function name(x)
            # TODO
            x
        end
        """),
        CodeTemplate(name: "For Loop", template: """
        for i in 1:N
            # TODO
        end
        """),
        CodeTemplate(name: "While Loop", template: """
        while condition
            # TODO
        end
        """),
        CodeTemplate(name: "If-Else", template: """
        if condition
            # true branch
        else
            # false branch
        end
        """),
    ]
}
```

**Checklist**:
- [ ] Define code templates
- [ ] Create template picker UI
- [ ] Insert template at cursor
- [ ] Allow custom templates

## Acceptance Criteria

- [ ] Execution history tracks last 50 runs
- [ ] Share sheet works for code export
- [ ] Code templates insert correctly
- [ ] Performance remains good with history
