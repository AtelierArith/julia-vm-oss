# iOS アプリ現状分析

**最終更新**: 2026-01-07 (Debugタブ/補完/REPL改善/サンプル更新反映)

---

## 実装状況サマリ

| フェーズ | 内容 | 状態 |
|---------|------|------|
| Phase 0 | Foundation & Architecture | ✅ 完了 |
| Phase 1 | Error Handling UI | ✅ 実装済み（UI検証未） |
| Phase 2 | Editor Enhancements | ✅ 実装済み |
| Phase 3 | Code Persistence | ❌ 未着手（モデルのみ） |
| Phase 4 | Settings | ❌ 未着手 |
| Phase 5 | iPad Optimization | ❌ 未着手 |
| Phase 6 | Advanced Features | ❌ 未着手（オプション） |

### REPL モード

| フェーズ | 内容 | 状態 |
|---------|------|------|
| REPL Phase A | Basic REPL UI | ✅ 完了 |
| REPL Phase B | Session Persistence | ✅ 完了 |
| REPL Phase C | Advanced Features | ⏳ 一部未実装（ストリーミングはEditorのみ） |

詳細: [REPL_IMPLEMENTATION.md](REPL_IMPLEMENTATION.md)

---

## 完了済み機能

### 基盤 / アーキテクチャ

- プロジェクト構成の再編（App / Models / Services / Views / Resources）
- 既存コードの移動と import 更新
- ビルド成功の確認
- AppConfiguration（アプリ名/実行制限/Editor 定数/テーマ色）
- Color hex 初期化拡張
- 外部依存なしの MVP 方針決定
- カスタムシンタックスハイライト基盤の実装
- モデル定義（CodeSample / ExecutionResult / VMError / UserScript / TestResult ほか）
- CodeSample の Codable 検証 & JSON + .jl ファイル読み込み
- 52 サンプルコード（11 カテゴリ）追加

### Editor モード

- UITextView ベースのモノスペースエディタ
- Monokai テーマのシンタックスハイライト
- 正規表現ベースのトークン色分け（キーワード/文字列/数値/コメント/関数）
- バックグラウンドでのハイライト処理
- 行番号表示（LineNumberGutterView）
- 日本語 IME 入力対応
- 出力テキスト選択（コピー可能）
- コードコピーボタン
- サンプルピッカー（難易度絵文字 + 説明表示）
- エディタ/出力のリサイズ（ドラッグ式ディバイダ）
- コード補完（キーワード/関数/変数、ポップアップ + Tab 適用）
- Unicode 補完（`\alpha` → `α` など、ポップアップ + Tab 適用）
- ストリーミング実行（println をリアルタイム表示）
- Stop ボタンによる実行キャンセル（VM cancel）
- ErrorView / ErrorBanner 統合表示
- StdIORedirector は実装済みだが現在は無効化（重複出力対策）

### Error Handling UI（Phase 1 実装分）

- `compile_and_run_detailed` の Swift FFI 定義
- C 構造体定義（CSpan / CErrorKind / CError / CExecutionResult）
- SourceSpan 変換（start/end/line/column）
- SourceSpan のスニペット抽出（前後コンテキスト）
- VMError モデル（種別 enum / 表示プロパティ）
- ErrorView コンポーネント（詳細表示）
- ErrorBanner コンポーネント（簡易表示）
- ContentView へのエラー表示統合
- VMErrorKindParser によるエラー種別パース

### REPL モード

#### Phase A（Basic REPL UI）

- Editor/REPL/Debug タブ切替（セグメントコントロール）
- `julia>` プロンプト表示
- Enter/Run での評価
- 履歴表示（ScrollView + 自動スクロール）
- ↑↓ボタンで履歴ナビゲーション
- Clear/Copy All
- エラーの赤色表示
- 実行時間表示
- タブ切替時の REPL 状態保持

#### Phase B（Session Persistence）

- REPL セッション FFI（`repl_session_new/eval/free/reset` + `free_repl_result`）
- REPLSessionManager（Swift 側）
- セッション初期化と解放
- eval 結果のパースとメモリ解放
- `ans` の永続化
- Reset Session 操作

#### Phase C（Advanced Features: 実装済み分）

- 不完全式の検出（`is_expression_complete` FFI）
- 複数行入力 UI（継続プロンプト）
- Enter で完了判定
- 入力/出力の構文ハイライト
- セミコロン抑制や複数式の分割実行（`split_expressions`）
- 関数/構造体定義の Julia 互換表示（generic function/struct）
- ハードウェアキーボードショートカット（↑↓/Ctrl+C/Ctrl+L/Ctrl+R）
- 履歴検索ビュー（History Search View）
- コンテキストメニューでのコピー機能（REPL エントリ）
- Stop ボタンによる実行キャンセル
- Unicode 補完（REPL 入力）
- Alt+Enter で改行 / ペーストでの複数行検出

### UI / UX

- ローンチスクリーン（アプリロゴ）
- スプラッシュ表示 + プログレスインジケータ
- ローディング画面へのロゴ追加

### Debug / QA

- Debug タブ（全サンプルの一括実行 UI）
- SampleTestRunner による順次実行/失敗再実行
- 進捗バー + サマリ表示（Passed/Failed/Total）
- 結果詳細の折りたたみ表示

---

## 未完了機能

- ⏳ Error handling UI（UI 検証）
- ❌ Code persistence（UserScript はモデルのみ）
- ❌ Settings UI
- ❌ iPad split view optimization
- ❌ REPL ストリーミング出力（Editor のみ対応）

---

## プロジェクト構造

```
SubsetJuliaVMApp/SubsetJuliaVMApp/
├── App/
│   ├── SubsetJuliaVMAppApp.swift
│   └── AppConfiguration.swift      # AppMode enum 含む
├── Models/
│   ├── CodeSample.swift            # サンプル定義
│   ├── CodeSamples+Beginner.swift  # 初級サンプル
│   ├── CodeSamples+Intermediate.swift  # 中級サンプル
│   ├── CodeSamples+Advanced.swift  # 上級サンプル
│   ├── REPLEntry.swift             # REPL 履歴エントリ
│   ├── TestResult.swift            # Debug 実行結果
│   ├── UserScript.swift            # 定義のみ
│   ├── ExecutionResult.swift
│   └── VMError.swift               # 基本モデル
├── Services/
│   ├── CodeCompletion/
│   │   └── CodeCompletionProvider.swift
│   ├── FFI/
│   │   ├── REPLSessionManager.swift
│   │   ├── UnicodeHelper.swift
│   │   ├── VMBridge.swift
│   │   └── StdIORedirector.swift
│   └── SampleTestRunner.swift
├── Views/
│   ├── ContentView.swift           # メイン (Editor/REPL 切替)
│   ├── ErrorView.swift
│   ├── Debug/
│   │   └── DebugView.swift
│   ├── Editor/
│   │   ├── MonospacedTextEditor.swift
│   │   ├── CodeCompletionView.swift
│   │   └── UnicodeCompletionView.swift
│   ├── REPL/
│   │   ├── REPLView.swift          # REPL メインビュー
│   │   ├── REPLEntryView.swift     # 履歴エントリ表示
│   │   ├── REPLInputView.swift     # 入力エリア
│   │   └── SyntaxHighlightedInput.swift
└── Resources/
```

---

## 依存関係

### Rust VM アーキテクチャ

iOS アプリは **Pure Rust パーサー** (`subset_julia_vm_parser`) を使用：
- ✅ Native/iOS/Web で完全に同一のコードパス
- ✅ Base ライブラリは起動時に Pure Rust パーサーでパース（`base_loader.rs`）
- ✅ プリコンパイル JSON は削除済み（起動時パース方式に移行）
- ✅ tree-sitter 依存は optional（デフォルトは Pure Rust パーサー）

### FFI 状況

Phase 1 (Error Handling UI) は `compile_and_run_detailed` FFI を利用：
- ✅ Rust側実装済み（`lib.rs`）
- ✅ Swift側FFI実装済み（`VMBridge.swift`）
- ✅ Editor UI への統合済み（UI検証待ち）

Editor 実行は `compile_and_run_streaming` を利用：
- ✅ 逐次出力（println のリアルタイム表示）
- ✅ 実行完了時の結果集約

キャンセル API（動的ロード）：
- ✅ `vm_request_cancel` / `vm_reset_cancel` を dlsym で解決（存在すれば使用）

REPL Phase B (Session Persistence) は FFI 実装済み：
- ✅ `repl_session_new` - セッション作成
- ✅ `repl_session_eval` - セッション内評価
- ✅ `repl_session_free` - セッション破棄
- ✅ `repl_session_reset` - セッションリセット
- ✅ `is_expression_complete` - 不完全式判定
- ✅ `split_expressions` - 複数式分割

Unicode 補完 FFI：
- ✅ `unicode_lookup` / `unicode_completions`
- ✅ `unicode_expand` / `unicode_reverse_lookup`

### 独立して開始可能

- Phase 3 (Persistence) - UserScript の保存/読み込み
- Phase 4 (Settings) - ユーザー設定
- Phase 5 (iPad) - Split View / iPad 向け UI
- REPL ストリーミング出力

---

## UI 概要

### モード切替

```
┌─────────────────────────────────────┐
│  [ Editor ]  [ REPL ]  [ Debug ]    │  ← セグメントコントロール
├─────────────────────────────────────┤
│                                     │
│  (選択したモードのビュー)            │
│                                     │
└─────────────────────────────────────┘
```

### REPL 画面

```
┌─────────────────────────────────────┐
│ Julia REPL    [Copy][Search][Clear][Reset] │
├─────────────────────────────────────┤
│ julia> 1 + 2                        │
│ 3                                   │
│ [0.001 ms]                          │
│                                     │
│ julia> println("Hello")             │
│ Hello                               │
│ [0.002 ms]                          │
├─────────────────────────────────────┤
│ julia> [input]  [↑][↓] [Stop] [Run] │
└─────────────────────────────────────┘
```

---

## テスト状況

- **52 サンプルプログラム**の動作確認（Debug タブで一括実行）
- REPL モードの基本動作確認
- 単体テストは未整備

---

## 関連ドキュメント

- [OVERVIEW.md](./OVERVIEW.md) - iOS アプリ全体概要
- [COMPLETED_PHASES.md](./COMPLETED_PHASES.md) - 完了済みフェーズ
- [REPL_IMPLEMENTATION.md](./REPL_IMPLEMENTATION.md) - REPL 実装詳細
