# キーボード dismiss 用アクセサリバー(inputAccessoryView)設計

- 日付: 2026-07-05
- 対象: `SubsetJuliaVMApp`(iOS / SwiftUI + UIKit)
- 種別: 機能追加(UX)

## 背景 / 問題

Editor / REPL 画面でソフトウェアキーボードを表示すると、これを閉じる
(dismiss する)手段が UI 上に存在しない。

- コード入力なので `Return` は改行に割り当てられており、`Return` で閉じられない。
- 数字キーパッドと同様、「閉じる導線」を別途用意する必要がある。

iOS の標準解法は **input accessory view(キーボード上に載るツールバー)** に
「完了」ボタンを置くこと。Apple 純正アプリでもコードエディタ / 数値入力で定番。

## 現状構造(確認済み)

Editor / REPL とも SwiftUI の `TextEditor` ではなく、UIKit の
`UITextView` を `UIViewRepresentable` でラップした自前実装。

- Editor: `MonospacedTextEditor` / `LineNumberedTextEditor` → `EditorTextView`
  (`UITextView` subclass)。`MonospacedTextEditor.swift`。
  既存の `@objc` ハンドラ: `handleTab()` / `handleUpArrow()` / `handleDownArrow()`
  / `handleShiftTab()` など(現状 `keyCommand` 経由でのみ発火、`private`)。
- REPL: `SyntaxHighlightedTextEditor` → `REPLTextView`(`UITextView` subclass)。
  `SyntaxHighlightedInput.swift`。既存 Tab 経路あり(`handleTab()` /
  `handleTabButtonIfNeeded` / `tabActionRequest`)。1行版 `SyntaxHighlightedTextField`
  (`UITextField`)も存在。

→ SwiftUI 純正の `.toolbar { ToolbarItemGroup(placement: .keyboard) }` は
`UIViewRepresentable` の UITextView には効かない(focus 管理外)。UIKit の
`inputAccessoryView` を用いる。

## 確定要件(ユーザー合意)

1. **バーの内容**: フル機能 — `[Tab] [←] [→] …spacer… [完了]`。
2. **対象画面**: Editor と REPL の両方。
3. **背景タップでの dismiss**: 採用しない(バーの「完了」ボタンのみで閉じる)。

## 非目標(スコープ外)

- 入力エリア外の背景タップによる dismiss。
- Debug 画面(入力欄があっても今回は対象外)。
- `SyntaxHighlightedTextField`(1行版)。REPL の実入力に使われていなければ対象外。
  実装計画フェーズで使用有無を確定する。
- キーボード上バーへの補完候補表示・スニペット等の追加機能。

## アプローチ比較

| 案 | 内容 | 評価 |
|---|---|---|
| **A. 共通ファクトリ + 各 subclass にアクション委譲** | バー生成を1箇所に共通化。`EditorTextView` / `REPLTextView` に `@objc` アクションを追加し、既存の `handleTab()` 等へ委譲 | ✅ 採用 — 重複なし・両画面一貫・テスト可 |
| B. 各 subclass が個別に自前バー構築 | それぞれ `UIToolbar` を直接生成 | ❌ 重複・UI 不整合リスク |
| C. SwiftUI `.toolbar(.keyboard)` | 純正 API | ❌ 不可(UITextView は SwiftUI focus 管理外) |

**採用: 案A。**

## 設計詳細(案A)

### コンポーネント

1. **`KeyboardAccessoryBar`(新規・共通ファクトリ)**
   - `UIToolbar` を組み立てて返す。配置 `[Tab] [←] [→] …flexibleSpace… [完了]`。
   - ダークテーマ整合: `barStyle = .black`、tint を既存テーマ(Monokai / REPLTheme)に合わせる。
   - target / action を引数で受ける(呼び出し側の UITextView subclass を target に)。
   - 何を what: 見た目と配置だけを担う。ロジックを持たない。

2. **カーソル移動の共通化 `extension UITextInput { func moveCaret(by offset: Int) }`**
   - `UITextView` も `UITextField` も `UITextInput` 準拠 → 1実装で両対応。
   - `position(from:offset:)` が境界外で `nil` を返す性質を使い端でクランプ(no-op)。
   - 依存 what: `selectedTextRange` と `position(from:offset:)` のみ。UITextInput 実体で単体テスト可能。

3. **`EditorTextView` / `REPLTextView` へのアクション追加**
   - `@objc func accessoryTab()` → 既存 `handleTab()` に委譲。
   - `@objc func accessoryMoveLeft()` → `moveCaret(by: -1)`。
   - `@objc func accessoryMoveRight()` → `moveCaret(by: +1)`。
   - `@objc func accessoryDone()` → `resignFirstResponder()`。
   - バー設置: 各 `makeUIView` で
     `textView.inputAccessoryView = KeyboardAccessoryBar.make(target: textView, …)`。

### データフロー

```
バーのボタン tap
  → UITextView subclass の @objc アクション
    → 既存 Tab ロジック / selectedTextRange 更新(moveCaret) / resignFirstResponder
```

### エラーハンドリング / 境界

- カーソル移動は文書端でクランプ(何もしない)。
- first responder でない状態でボタンが押されても副作用なし。
- 既存の Tab / インデント挙動は変更しない(委譲のみ)。

## テスト計画

XCTest。既存 `SubsetJuliaVMApp/SubsetJuliaVMAppTests/`(例: `REPLFontSizeUpdateTests`)と同じ場所に1ファイル追加。

- `moveCaret(by:)`:
  - 先頭で `←` → 動かない(境界クランプ)。
  - 末尾で `→` → 動かない(境界クランプ)。
  - 中間で `←` / `→` → 1文字分だけ移動。
- `KeyboardAccessoryBar.make(...)`:
  - `items` に「完了」ボタンが含まれる。
  - `Tab` / `←` / `→` が期待順に並ぶ。
  - 完了ボタンの action が `resignFirstResponder` 相当に配線されている。

手動確認(実機 / シミュレータ):
- Editor / REPL でキーボード表示 → バーの「完了」で閉じる。
- `Tab` が既存の補完 / インデントと同じ挙動。
- `←` / `→` でキャレットが1文字ずつ移動。

## 対象ファイル

- 新規: `Views/.../KeyboardAccessoryBar.swift`(共通ファクトリ)。
  `UITextInput+MoveCaret.swift` を同居 or 別ファイルで追加。
- 変更: `Views/Editor/MonospacedTextEditor.swift`(`EditorTextView`)。
- 変更: `Views/REPL/SyntaxHighlightedInput.swift`(`REPLTextView`)。
- 新規テスト: `SubsetJuliaVMAppTests/KeyboardAccessoryBarTests.swift`(仮)。

## 未確定事項(実装計画で確定)

- 新規ファイルの正確な配置ディレクトリ(`Views/Shared/` 等の既存慣習に合わせる)。
- REPL の実入力に `SyntaxHighlightedTextField`(1行 `UITextField`)が使われているか。
  使われていれば同様にアクセサリバーを付与するか判断。
- 既存 `handleTab()` が `private` のため、同一 subclass 内でのラッパー化 or アクセス緩和の要否。
- iOS の Issue 起票要否(新機能のため必須ではないが、リポジトリ慣習に従い検討)。
