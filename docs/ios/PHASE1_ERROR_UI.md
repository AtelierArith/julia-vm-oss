# Phase 1: Error Handling UI

**Goal**: 包括的なエラー表示システムの実装

**Status**: ⏳ **UI検証待ち**

**Priority**: Critical

## 依存関係

Phase 1 は Rust VM の `compile_and_run_detailed` FFI に依存:
- ✅ Rust側実装済み（`lib.rs`）
- ✅ Swift側FFI実装済み（`VMBridge.swift`）
- ⏳ Editor UI への表示統合が未完了

## Task 1.1: Update Swift FFI for Detailed Errors

**File**: `Services/FFI/VMBridge.swift`

### C 構造体定義

```swift
// MARK: - C Structs (matching subset_vm.h)
struct CSpan {
    var start: UInt32
    var end: UInt32
    var startLine: UInt32
    var endLine: UInt32
    var startColumn: UInt32
    var endColumn: UInt32
}

enum CErrorKind: Int32 {
    case none = 0
    case syntax = 1
    case unsupported = 2
    case runtime = 3
    case compile = 4
}

struct CError {
    var kind: Int32
    var span: CSpan
    var message: UnsafeMutablePointer<CChar>?
    var hint: UnsafeMutablePointer<CChar>?
}

struct CExecutionResult {
    var success: Bool
    var resultValue: Double
    var output: UnsafeMutablePointer<CChar>?
    var error: CError
}

@_silgen_name("compile_and_run_detailed")
func compile_and_run_detailed(_ src: UnsafePointer<CChar>, _ seed: UInt64) -> UnsafeMutablePointer<CExecutionResult>?

@_silgen_name("free_string")
func free_string(_ ptr: UnsafeMutablePointer<CChar>?)

@_silgen_name("free_execution_result")
func free_execution_result(_ result: UnsafeMutablePointer<CExecutionResult>?)
```

### SourceSpan ヘルパー

```swift
struct SourceSpan {
    let start: UInt32
    let end: UInt32
    let startLine: UInt32
    let endLine: UInt32
    let startColumn: UInt32
    let endColumn: UInt32

    init(from cSpan: CSpan) {
        self.start = cSpan.start
        self.end = cSpan.end
        self.startLine = cSpan.start_line
        self.endLine = cSpan.end_line
        self.startColumn = cSpan.start_column
        self.endColumn = cSpan.end_column
    }

    func range(in source: String) -> Range<String.Index>? {
        // UTF-16 ベースの範囲変換
    }

    func snippet(from source: String, context: Int = 2) -> String {
        // エラー行周辺のスニペット抽出
    }
}
```

**Checklist**:
- [x] Define C structs in Swift
- [x] Add `compile_and_run_detailed` FFI binding
- [x] Implement SourceSpan helpers
- [x] Create VMBridge wrapper class
- [ ] Test FFI calls

## Task 1.2: VMError Model

**File**: `Models/VMError.swift`

```swift
enum VMErrorKind: Equatable {
    case syntax
    case unsupportedFeature(UnsupportedFeature)
    case runtime(RuntimeErrorType)
}

enum UnsupportedFeature: String, CaseIterable {
    case macroCall = "Macro call"
    case macroDefinition = "Macro definition"
    case usingStatement = "using statement"
    // ...
}

struct VMError: Identifiable {
    let id = UUID()
    let kind: VMErrorKind
    let span: SourceSpan
    let message: String
    let hint: String?
    let source: String

    var title: String { ... }
    var icon: String { ... }
    var color: Color { ... }
    var snippet: String { ... }
}
```

**Checklist**:
- [x] Define VMError struct
- [x] Implement error kind enums
- [x] Add FFI conversion logic
- [x] Add helper properties (title, icon, color)

## Task 1.3: ErrorView Component

**File**: `SubsetJuliaVMApp/SubsetJuliaVMApp/Views/ErrorView.swift`

### 主要コンポーネント

1. **ErrorView** - 詳細なエラー表示
2. **ErrorBanner** - コンパクトなインラインバナー

### ErrorView 機能

- エラーアイコンとタイトル
- 行・列情報
- コードスニペット（エラー箇所ハイライト）
- ヒントメッセージ
- 展開/折りたたみ（未実装）

```swift
struct ErrorView: View {
    let error: VMError
    @State private var showFullDetails = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Header with icon and title
            // Error message
            // Expandable details (snippet, hint)
        }
    }
}
```

**Checklist**:
- [x] Create ErrorView component
- [x] Add collapsible details
- [x] Create ErrorBanner component
- [x] Add preview data
- [ ] Test with different error types

## Task 1.4: Integrate Error Display into ContentView

ContentView を更新してエラーシステムを統合:

- エラーバナー表示
- ErrorView での詳細表示
- アニメーション遷移

**Checklist**:
- [x] Integrate ErrorView
- [x] Add error banner
- [ ] Update button states
- [ ] Test error transitions

## Acceptance Criteria

- [x] Rust VM returns detailed error info
- [x] VMError model correctly parses C structs
- [ ] ErrorView displays all error types beautifully
- [ ] Errors show source span with line/column
- [ ] Code snippets visible with context
- [ ] Hints displayed when available
- [ ] Error banner appears/dismisses smoothly
- [ ] All 3 error types tested (syntax, unsupported, runtime)
