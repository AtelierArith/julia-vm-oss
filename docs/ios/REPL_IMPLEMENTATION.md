# REPL モード実装ガイド

Julia REPL.jl の仕様を iOS で再現した対話型実行環境の実装詳細。

**最終更新**: 2025-12-30

---

## 目次

1. [概要](#概要)
2. [アーキテクチャ](#アーキテクチャ)
3. [実装フェーズ](#実装フェーズ)
4. [Phase C 詳細](#phase-c-詳細)
5. [技術的課題](#技術的課題)
6. [将来の拡張](#将来の拡張)

---

## 概要

iOS アプリに Editor モードに加えて REPL モードを追加し、タブで切り替え可能にする。
Julia の REPL.jl を参考に、iOS に適した形で対話型実行環境を実装する。

### Status

| フェーズ | 内容 | 状態 |
|---------|------|------|
| Phase A | Basic REPL UI | ✅ 完了 |
| Phase B | Session Persistence | ✅ 完了 |
| Phase C | Advanced Features | ⏳ 部分完了 |

### Julia REPL の主要機能

| 機能 | 説明 | iOS実装 |
|------|------|---------|
| `julia>` プロンプト | 入力待ち状態の表示 | ✅ |
| コマンド履歴 | ↑↓で過去の入力呼び出し | ✅ |
| 出力表示 | 評価結果の表示 | ✅ |
| 不完全式の継続入力 | `if true` → 継続待ち | ✅ |
| `ans` 変数 | 前回の結果を参照 | ✅ |
| 構文ハイライト | 入力中のコード着色 | ✅ |

---

## アーキテクチャ

### UI 構造

```
┌─────────────────────────────────────────────────┐
│              ContentView                         │
│  ┌──────────────────────────────────────────┐   │
│  │  Picker (Segmented Control)               │   │
│  │  [Editor] [REPL]                          │   │
│  └──────────────────────────────────────────┘   │
│                                                  │
│  ┌──────────────────────────────────────────┐   │
│  │  EditorView (existing)                    │   │
│  │  or                                       │   │
│  │  REPLView (new)                           │   │
│  └──────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
```

### REPL 画面構成

```
┌─────────────────────────────────────┐
│ History (scrollable)                │
│ ─────────────────────────────────── │
│ julia> println("Hello")             │
│ Hello                               │
│                                     │
│ julia> 1 + 2                        │
│ 3                                   │
│                                     │
│ julia> x = [1, 2, 3]                │
│ 3-element Vector{Int64}:            │
│  1                                  │
│  2                                  │
│  3                                  │
│                                     │
├─────────────────────────────────────┤
│ julia> [input field]          [Run] │
└─────────────────────────────────────┘
```

### 関連ファイル

```
SubsetJuliaVMApp/Views/REPL/
├── REPLView.swift          # REPL メインビュー
├── REPLEntryView.swift     # 履歴エントリ表示
└── REPLInputView.swift     # 入力エリア
```

---

## 実装フェーズ

### Phase A: Basic REPL UI ✅

- Editor/REPL タブ切替（セグメントコントロール）
- `julia>` プロンプト表示
- Enter/Run での評価
- 履歴表示（ScrollView + 自動スクロール）
- ↑↓ボタンで履歴ナビゲーション
- Clear/Copy All
- エラーの赤色表示
- 実行時間表示
- タブ切替時の REPL 状態保持

### Phase B: Session Persistence ✅

**Rust FFI（実装済み）**:
```rust
// セッション管理
repl_session_new() -> *mut REPLSession
repl_session_eval(session: *mut REPLSession, input: &str) -> *mut CREPLResult
repl_session_reset(session: *mut REPLSession)
repl_session_free(session: *mut REPLSession)
free_repl_result(result: *mut CREPLResult)
```

**Swift 側（実装済み）**:
```swift
class REPLSessionManager {
    private var session: OpaquePointer?

    init(seed: UInt64)
    func eval(code: String) -> ExecutionResult
    func reset()
}
```

- セッション初期化と解放
- eval 結果のパースとメモリ解放
- `ans` の永続化
- Reset Session 操作

---

## Phase C 詳細

### Task C.1: 不完全式の検出 ✅

**Rust FFI（実装済み）**:
```rust
/// 式が完全かどうかを判定
#[no_mangle]
pub extern "C" fn is_expression_complete(src: *const c_char) -> i32 {
    let src = unsafe { CStr::from_ptr(src).to_str().unwrap_or("") };

    match parse_julia(src) {
        Ok(cst) => (!is_incomplete(&cst)) as i32,
        Err(_) => 1, // Syntax error = complete (will show error)
    }
}
```

**Swift 側（実装済み）**:
```swift
@_silgen_name("is_expression_complete")
private func is_expression_complete(_ src: UnsafePointer<CChar>) -> Int32

extension REPLSessionManager {
    func isComplete(code: String) -> Bool {
        code.withCString { ptr in
            is_expression_complete(ptr)
        }
    }
}
```

### Task C.2: 複数行入力 UI ✅

```swift
struct REPLInputView: View {
    @Binding var inputText: String
    @Binding var isMultiline: Bool

    private func handleSubmit() {
        if sessionManager.isComplete(code: inputText) {
            onSubmit()
        } else {
            // 継続入力モードに切り替え
            isMultiline = true
            inputText += "\n"
        }
    }
}
```

継続プロンプト表示:
```
julia> if x > 0
       |  (継続入力待ち)
```

**Checklist**:
- [x] 複数行入力モード
- [x] 継続プロンプト表示
- [x] Enter で完了判定
- [ ] Shift+Enter で強制改行

### Task C.3: 入力中の構文ハイライト ✅

既存の Monokai テーマを TextField に適用。

### Task C.4: キーボードショートカット ✅

```swift
.onKeyPress(.upArrow) {
    onHistoryPrev()
    return .handled
}
.onKeyPress(.downArrow) {
    onHistoryNext()
    return .handled
}
.onKeyPress("c", modifiers: .control) {
    // Cancel current input
    inputText = ""
    return .handled
}
.onKeyPress("l", modifiers: .control) {
    // Clear screen
    history.removeAll()
    return .handled
}
```

**Checklist**:
- [x] ↑↓ で履歴ナビゲーション
- [x] Ctrl+C で入力キャンセル
- [x] Ctrl+L で画面クリア

### Task C.5: 履歴検索

```swift
struct HistorySearchView: View {
    @Binding var searchText: String
    let history: [String]
    @Binding var selectedIndex: Int

    var filteredHistory: [String] {
        if searchText.isEmpty {
            return history
        }
        return history.filter { $0.contains(searchText) }
    }
}
```

**Checklist**:
- [ ] 履歴検索 UI
- [ ] インクリメンタルフィルタ
- [ ] 選択で入力欄に挿入

### Task C.6: 出力フォーマット

型に応じた整形表示:

```swift
struct FormattedOutputView: View {
    let value: REPLValue

    var body: some View {
        switch value {
        case .array(let elements):
            ArrayOutputView(elements: elements)
        case .matrix(let rows, let cols, let data):
            MatrixOutputView(rows: rows, cols: cols, data: data)
        case .number(let n):
            Text(String(n))
        case .string(let s):
            Text("\"\(s)\"")
        }
    }
}
```

**Checklist**:
- [ ] 配列の整形表示
- [ ] 行列の整形表示
- [ ] 型情報の表示

---

## 技術的課題

### 1. VM セッション維持（Phase B）

REPL セッションにより変数・関数・`ans` が評価間で保持される。

### 2. 不完全式の検出（Phase C）

`is_expression_complete` FFI で式が完全かどうかを判定。

### 3. iOS キーボード対応

- ハードウェアキーボード: ↑↓でヒストリ操作
- ソフトウェアキーボード: ヒストリボタンを追加

---

## 将来の拡張（オプション）

### ヘルプモード

```
julia> ?println
  println([io::IO], xs...)

  Print (using print) xs to io followed by a newline.
```

### タブ補完

```
julia> pri[TAB]
print    println
```

---

## 関連ドキュメント

- [Julia REPL.jl](https://github.com/JuliaLang/julia/tree/master/stdlib/REPL) - 参考実装
- [OVERVIEW.md](./OVERVIEW.md) - iOS アプリ全体概要
- [COMPLETED_PHASES.md](./COMPLETED_PHASES.md) - 完了済みフェーズ
- [STATUS.md](./STATUS.md) - 現在の状況
