# ✅ Completed: Remove Precompiled base.json

**Status**: ✅ **実装完了** (2025-12-30)

## Background

以前、Base 標準ライブラリ関数は `base.json` にプリコンパイルされ、WASM バイナリに埋め込まれていました。これは tree-sitter (C ライブラリ) が WASM に直接コンパイルできなかったためです。

Pure Rust パーサー (`subset_julia_vm_parser`) の導入により、Base ライブラリも起動時にパースする方式に移行しました。

## 以前のアーキテクチャ

```
[User Code]
JavaScript (web-tree-sitter) → CST JSON → WASM (Lowering + Execute)

[Base Library]
Precompiled base.json (embedded in WASM) → Deserialize → Merge with user program
```

## 実装されたアーキテクチャ

```
[User Code + Base Library]
Pure Rust Parser (WASM) → Lowering → Compile → VM
```

ユーザーコードと Base ライブラリが完全に同一のパイプラインを使用します。

## 実装内容

### 実装された方式

Pure Rust パーサーを使用して Base ライブラリを起動時にパースする方式を採用しました。

#### 1. Base Loader モジュール (`subset_julia_vm/src/base_loader.rs`)

```rust
pub fn get_base_program() -> Option<&'static Program> {
    // Pure Rust パーサーで Base をパース
    static BASE_PROGRAM: Lazy<Option<Program>> = Lazy::new(|| {
        let source = crate::base::get_base();

        // Pure Rust パーサーでパース
        let mut parser = Parser::new().expect("Failed to create parser");
        let parse_outcome = parser.parse(&source)?;

        // 統一 Lowering で IR に変換
        let mut lowering = Lowering::new(&source);
        lowering.lower(parse_outcome)
    });

    BASE_PROGRAM.as_ref()
}
```

#### 2. プリコンパイル JSON の削除

- ✅ `subset_julia_vm/src/precompiled/` ディレクトリを削除
- ✅ `base.json`, `statistics.json` などのプリコンパイルファイルを削除
- ✅ 起動時パース方式に移行

#### 3. 統一パイプライン

```
Base Library (.jl files)
  ↓
Pure Rust Parser (WASM)
  ↓
Lowering (統一コードパス)
  ↓
Core IR
  ↓
Merge with user program
```

## 実装されたファイル

1. ✅ `subset_julia_vm/src/base_loader.rs` - Base プログラムの起動時パース
2. ✅ `subset_julia_vm/src/lib.rs` - `get_base_program()` を使用
3. ✅ `subset_julia_vm/src/precompiled/` - 削除済み

## 実装結果

### 達成された利点

- ✅ **シンプルなアーキテクチャ**: ユーザーコードと Base ライブラリが同一パイプライン
- ✅ **メンテナンス性向上**: `.jl` ファイル変更時に JSON 再生成不要
- ✅ **WASM バイナリサイズ削減**: 埋め込み JSON なし
- ✅ **Native/Web 統一**: 完全に同一のコードパス

### パフォーマンス

- Base ライブラリのパースは起動時に一度だけ実行（Lazy 初期化）
- パース時間は許容範囲内（数百ミリ秒程度）
- 実行時パフォーマンスへの影響なし

### テスト結果

- ✅ 既存の fixture tests がすべてパス
- ✅ Web Playground で各種サンプルが正常動作
- ✅ 起動時間は許容範囲内

## 関連ドキュメント

- `docs/vm/STATUS.md` - 実装状況の詳細
- `docs/web/WEB_ARCHITECTURE.md` - Web アーキテクチャの全体像
- `subset_julia_vm/src/base_loader.rs` - 実装コード
