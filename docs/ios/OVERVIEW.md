# SubsetJuliaVMApp - iOS 実装概要

**Platform**: iOS (SwiftUI)
**Minimum Target**: iOS 16.0
**Primary Focus**: User experience, error display, code management
**VM Architecture**: Pure Rust パーサー（Native/Web と同一パイプライン）

## 概要

このドキュメントは `../implementation/IMPLEMENTATION_PLAN.md`（Rust VM フォーカス）を補完し、**iOS アプリ実装**の詳細を記述します。アプリは基本的なコード実行環境から、洗練された Julia サブセット IDE へと進化することを目指します。

## 設計原則

1. **Native iOS Experience** - Apple Human Interface Guidelines に準拠
2. **Error-First Design** - エラーは学習機会（ヒントを表示）
3. **Educational Focus** - Julia サブセットの制限を理解しやすく
4. **iPad-Optimized** - 大画面を活用
5. **Offline-First** - コア機能にネットワーク不要

## アプリモード

アプリは2つのモードを持ち、セグメントコントロールで切り替え可能：

| モード | 説明 | 状態 |
|--------|------|------|
| **Editor** | サンプルコード選択・編集・実行 | ✅ 実装済み |
| **REPL** | Julia 風の対話型実行環境 | ✅ Phase A 完了 |

```
┌─────────────────────────────────────┐
│  [ Editor ]  [ REPL ]               │
├─────────────────────────────────────┤
│                                     │
│  (選択したモードのビュー)            │
│                                     │
└─────────────────────────────────────┘
```

## アーキテクチャ

```
┌─────────────────────────────────────────────────┐
│                  SwiftUI Views                   │
├─────────────────────────────────────────────────┤
│  ContentView (Mode Switch)                       │
│  ├── Editor Mode                                 │
│  │   └── MonospacedTextEditor, Sample Picker     │
│  └── REPL Mode                                   │
│      └── REPLView, REPLEntryView, REPLInputView  │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────┐
│                  Services                        │
│   VMBridge │ StdIORedirector                     │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────┴──────────────────────────┐
│               Rust VM (via FFI)                  │
│   Pure Rust Parser → Lowering → Compile → VM    │
│   compile_and_run │ compile_and_run_detailed     │
│   repl_session_* (REPL セッション管理)           │
└─────────────────────────────────────────────────┘
```

### VM パイプライン

iOS アプリは **Pure Rust パーサー** (`subset_julia_vm_parser`) を使用し、Native/Web と完全に同一のパイプラインを実現：

```
Julia source (Swift)
    ↓
Rust VM (Pure Rust Parser)
    ↓
Lowering → Core IR
    ↓
Compiler → Bytecode
    ↓
VM Execution
    ↓
Results (via C ABI)
```

**特徴**:
- Base ライブラリは起動時に Pure Rust パーサーでパース（`base_loader.rs`）
- プリコンパイル JSON は不要（起動時パース方式）
- Native/iOS/Web で完全に同一のコードパス

## FFI インターフェース

### 現在利用可能

```swift
@_silgen_name("compile_and_run")
func compile_and_run(_ src: UnsafePointer<CChar>, _ seed: UInt64) -> Double

@_silgen_name("compile_and_run_with_output")
func compile_and_run_with_output(_ src: UnsafePointer<CChar>, _ seed: UInt64) -> UnsafePointer<CChar>?

@_silgen_name("compile_to_ir")
func compile_to_ir(_ src: UnsafePointer<CChar>) -> UnsafePointer<CChar>?

@_silgen_name("free_string")
func free_string(_ ptr: UnsafeMutablePointer<CChar>?)
```

### 詳細エラー用（Rust/Swift 実装済み）

```swift
@_silgen_name("compile_and_run_detailed")
func compile_and_run_detailed(_ src: UnsafePointer<CChar>, _ seed: UInt64) -> UnsafeMutablePointer<CExecutionResult>?

@_silgen_name("free_execution_result")
func free_execution_result(_ result: UnsafeMutablePointer<CExecutionResult>)
```

### REPL セッション用（実装済み）

```swift
@_silgen_name("repl_session_new")
func repl_session_new(_ seed: UInt64) -> OpaquePointer?

@_silgen_name("repl_session_eval")
func repl_session_eval(_ session: OpaquePointer?, _ src: UnsafePointer<CChar>) -> UnsafeMutablePointer<CREPLResult>?

@_silgen_name("repl_session_free")
func repl_session_free(_ session: OpaquePointer?)

@_silgen_name("repl_session_reset")
func repl_session_reset(_ session: OpaquePointer?)

@_silgen_name("free_repl_result")
func free_repl_result(_ result: UnsafeMutablePointer<CREPLResult>?)
```

## 実装フェーズ一覧

### 基本フェーズ

| ドキュメント | 内容 | ステータス |
|------------|------|-----------|
| [02_PHASE0.md](02_PHASE0.md) | Foundation & Architecture | ✅ 完了 |
| [03_PHASE1.md](03_PHASE1.md) | Error Handling UI | ⏳ UI検証待ち |
| [04_PHASE2.md](04_PHASE2.md) | Editor Enhancements | ⏳ 部分完了 |
| [05_PHASE3.md](05_PHASE3.md) | Code Persistence | ❌ 未着手 |
| [06_PHASE4.md](06_PHASE4.md) | Settings | ❌ 未着手 |
| [07_PHASE5.md](07_PHASE5.md) | iPad Optimization | ❌ 未着手 |
| [08_PHASE6.md](08_PHASE6.md) | Advanced Features | ❌ オプション |
| [09_TESTING.md](09_TESTING.md) | テスト戦略 | - |

### REPL フェーズ

| ドキュメント | 内容 | ステータス |
|------------|------|-----------|
| [10_REPL_OVERVIEW.md](10_REPL_OVERVIEW.md) | REPL 実装概要 | - |
| [11_REPL_PHASE_A.md](11_REPL_PHASE_A.md) | Basic REPL UI | ✅ 完了 |
| [12_REPL_PHASE_B.md](12_REPL_PHASE_B.md) | Session Persistence | ✅ 完了 |
| [13_REPL_PHASE_C.md](13_REPL_PHASE_C.md) | Advanced Features | ⏳ 部分完了 |

## 関連ドキュメント

- [../implementation/DESIGN.md](../implementation/DESIGN.md) - 設計思想とアーキテクチャ
- [../implementation/IMPLEMENTATION_PLAN.md](../implementation/IMPLEMENTATION_PLAN.md) - Rust VM 実装計画
- [../../CLAUDE.md](../../CLAUDE.md) - プロジェクト概要
