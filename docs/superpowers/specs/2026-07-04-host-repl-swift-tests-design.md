# Simulator 不要の host-side REPL 単体テスト (PoC)

- **Date**: 2026-07-04
- **Status**: Approved (design), pending implementation
- **Branch**: `feat/host-repl-swift-tests`

## 背景 / 動機

iOS アプリ `SubsetJuliaVMApp` は REPL タブで Julia コードを実行し結果を表示する。
REPL 実行経路 (`Services/FFI/VMBridge.swift` → C ABI `compile_and_run_detailed`
→ Rust VM) は **SwiftUI / WebKit に依存しない**。実際、既存の
`SubsetJuliaVMAppTests/REPL*.swift`（`REPLConcurrentEvalSafetyTests` 等）は
この実 FFI 経路を UI なしで叩いている。

しかし現在のテストターゲットは:

- `SDKROOT = iphoneos`・`TEST_HOST = …/SubsetJuliaVMApp.app`（アプリ hosted な
  unit-test bundle）。→ アプリを Simulator に入れて起動してからテストが走る。
- xcframework は iOS スライスのみ (`ios-arm64` / `ios-arm64-simulator`)。macOS
  スライスが無い。
- iOS SDK のテストバンドルは UI を出さなくても **iOS Simulator ランタイム上**で
  実行される。

そのため「Simulator を全く起動せずに REPL 実行の Swift 単体テストを回す」には、
**macOS host で `swift test` を走らせる**構成が必要。これには (A) Rust FFI の
host (`aarch64-apple-darwin`) 向けビルドと、(B) REPL コア層を host-runnable な
SwiftPM ターゲットへ切り出すこと、の2点が要る。

本 spec はその最小実証 (PoC) を定義する。

## ゴール

REPL 実行経路 (Swift → C ABI → Rust VM) を **iOS Simulator なしで
`swift test`（macOS host）から叩ける**ことを、host ビルドの静的ライブラリを
リンクするローカル SwiftPM パッケージで実証する。

完了条件:

1. `cargo build --release -p subset_julia_vm_ffi` で得た
   `target/release/libsubset_julia_vm.a` をリンクし、
2. `cd SubsetJuliaKit && swift test` が **Simulator 関与ゼロ**で緑になる。
3. `.a` 未ビルド時は分かりやすいリンクエラーで落ちる（＝隠れた成功に見えない）。

## 非ゴール (この PoC ではやらない)

- 既存 `SubsetJuliaVMAppTests/REPL*.swift` の SubsetJuliaKit への移送。
- アプリターゲット (`SubsetJuliaVMApp`) への変更・パッケージ依存化。
- CI ゲート追加 (`scripts/` スクリプト・`CODE_AUDITS.md` 登録)。
- Base バイトコードキャッシュの埋め込み（ランタイム逐次コンパイルで可）。

これらは PoC で配線が緑になった後の後続フェーズで扱う。

## アーキテクチャ

リポジトリ直下に新規ローカル SwiftPM パッケージ `SubsetJuliaKit/` を追加する。
**アプリのソースは import せず、必要な FFI 宣言だけを抜粋コピー**する。これで
PoC 中はアプリのビルドに一切影響を与えない（リスク遮断）。本物の
`VMBridge` / `REPLSessionManager` の移送は後続フェーズ。

```
SubsetJuliaKit/
├── Package.swift              # macOS host ターゲット。library + test の2ターゲット
├── Sources/SubsetJuliaKit/
│   └── VMBridgeCore.swift     # 最小 FFI 層
├── Tests/SubsetJuliaKitTests/
│   └── REPLSmokeTests.swift   # host スモークテスト
└── run_tests.sh              # cargo build → swift test を1発で
```

### `Package.swift`

- `platforms: [.macOS(.v13)]`（host 実行）。
- library ターゲット `SubsetJuliaKit`。
- test ターゲット `SubsetJuliaKitTests`（`SubsetJuliaKit` 依存）。
- library ターゲットに
  `linkerSettings: [.unsafeFlags(["-L../target/release", "-lsubset_julia_vm"])]`。
  - `[lib] name = "subset_julia_vm"` なので成果物は `libsubset_julia_vm.a` →
    `-lsubset_julia_vm`。
  - CARGO_TARGET_DIR は既定で `<repo>/target`、パッケージは `<repo>/SubsetJuliaKit`
    なので相対 `../target/release` が host ビルド成果物を指す。
  - ローカル path 依存なので `unsafeFlags` の依存配布制限は無関係。

### `Sources/SubsetJuliaKit/VMBridgeCore.swift`

`SubsetJuliaVMApp/.../Services/FFI/VMBridge.swift` から必要最小限を抜粋:

- `@_silgen_name("compile_and_run_detailed")` /
  `@_silgen_name("free_execution_result")` の宣言。
- C ABI ミラー構造体 `CSpan` / `CErrorKind` / `CError` / `CExecutionResult`
  （**安定プレフィックス** `success`, `resultValue`, `output`, `error` のみ）。
- 薄い公開ラッパ:

  ```swift
  public struct JuliaRunResult {
      public let success: Bool
      public let value: Double?   // resultValue が NaN のとき nil
      public let output: String
      public let error: String?
  }
  public func runJulia(_ source: String, seed: UInt64 = 0) -> JuliaRunResult
  ```

  `runJulia` は `source.withCString → compile_and_run_detailed(cstr, seed) →
  .pointee 読取 → free_execution_result` を行う。アプリの `VMBridge.execute`
  と同一の呼び出し形。

### `Tests/SubsetJuliaKitTests/REPLSmokeTests.swift`

- `testOnePlusOne`: `runJulia("1 + 1")` が `success == true` かつ
  `value == 2.0`。
- `testSyntaxErrorReported`: 壊れた入力（例 `"1 +"`）が `success == false`。

### `run_tests.sh`

```sh
#!/usr/bin/env sh
set -eu
# repo root から host FFI をビルド
cargo build --release -p subset_julia_vm_ffi
# パッケージ内で host テスト
cd "$(dirname "$0")"
swift test
```

## データフロー

```
XCTest
  └─ runJulia("1 + 1")
       └─ source.withCString { cstr in
            compile_and_run_detailed(cstr, seed)  ── C ABI ──▶ Rust VM (host .a)
          }
       └─ resultPtr.pointee 読取 (success / resultValue / output / error)
       └─ free_execution_result(resultPtr)
       └─ JuliaRunResult を返す
```

## リスクと対策

| リスク | 対策 |
|---|---|
| `-L../target/release` の相対 path が解決されない | 実装時に `swift test` で実証。ダメなら env 経由で絶対 path 化するか、`run_tests.sh` で `-L$(pwd)/../target/release` を明示 |
| static lib の未参照シンボル除去 | Swift から `compile_and_run_detailed` を参照するので保持される。Rust staticlib は自己完結。リンク成功を実証 |
| Base をランタイム逐次コンパイル（キャッシュ無） | 初回のみ遅い。正しさは不変。遅すぎれば後続で `SJULIA_BASE_CACHE` を検討 |
| 新しい struct フィールドとの weak-link 不整合 | 安定プレフィックス (`success`/`resultValue`/`output`/`error`) のみ読む（アプリの既存方針と同一） |
| host `.a` が iOS 固有コードに依存してビルド不能 | FFI クレートは純 Rust。`VMBridge.swift` の cancel ルックアップも既に macOS 用 `RTLD_DEFAULT` 分岐を持つ（host 想定済み）。`cargo build -p subset_julia_vm_ffi` の成功で実証 |

## 検証

- `SubsetJuliaKit/run_tests.sh` 実行で `swift test` が緑。
- 実行中に iOS Simulator デバイスが起動しないこと（`xcrun simctl list booted` が
  空、もしくは手動で確認）。
- `target/release/libsubset_julia_vm.a` を消した状態では、`swift test` が
  未定義シンボルのリンクエラーで明確に失敗すること。

## 後続フェーズ（この PoC の外）

1. アプリの `VMBridge` / `REPLSessionManager` / 関連 Model 型を SubsetJuliaKit へ
   移送し、アプリを同パッケージ依存に切替。
2. 既存 `REPL*.swift` テストを `@testable import SubsetJuliaKit` で移送。
3. `scripts/` に host テストスクリプトを追加、`docs/vm/CODE_AUDITS.md` に登録して
   CI ゲート化。
