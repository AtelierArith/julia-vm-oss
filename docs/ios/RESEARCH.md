# iOS リサーチドキュメント

iOS アプリ実装に関する技術調査・比較検討結果のまとめ。

---

## 目次

1. [Unicode 入力支援システム](#unicode-入力支援システム)
2. [コードエディタ選定](#コードエディタ選定)

---

## Unicode 入力支援システム

Julia REPL の `\alpha` + Tab で `α` に変換する機能の調査と実装。

### Julia のアーキテクチャ

```
ユーザー入力: \alpha + Tab
    ↓
LineEdit.jl: Tab キーを検出
    ↓
REPLCompletions.jl: bslash_completions() 呼び出し
    ↓
latex_symbols.jl: Dict["\\alpha"] = "α" をルックアップ
    ↓
入力テキストを Unicode 文字に置換
```

### 主要ファイル（julia リポジトリ）

```
julia/stdlib/REPL/src/
├── latex_symbols.jl    # 主要マッピング (2,718行)
├── emoji_symbols.jl    # 絵文字
├── REPLCompletions.jl  # 補完ロジック
└── docview.jl          # 逆引き (ヘルプ用)
```

### 補完の種類

| 種類 | 例 | 結果 |
|------|-----|------|
| 基本シンボル | `\alpha` | α |
| ギリシャ大文字 | `\Alpha` | Α |
| 演算子 | `\times`, `\div` | ×, ÷ |
| 矢印 | `\rightarrow`, `\Rightarrow` | →, ⇒ |
| 下付き | `\_0`, `\_a` | ₀, ₐ |
| 上付き | `\^2`, `\^n` | ², ⁿ |
| 太字 | `\bfalpha` | 𝛂 |
| 黒板太字 | `\bbR`, `\bbN` | ℝ, ℕ |
| 筆記体 | `\scrA` | 𝒜 |
| 絵文字 | `\:smile:` | 😄 |

### SubsetJuliaVM 実装状況

#### 完了

- [x] `latex_symbols.jl` から必要なマッピングを抽出
- [x] Rust モジュール作成 (`subset_julia_vm/src/unicode.rs`)
- [x] FFI 関数追加 (`unicode_lookup`, `unicode_completions`, `unicode_expand`, `unicode_reverse_lookup`)
- [x] C ヘッダー更新 (`subset_julia_vm/include/subset_vm.h`)
- [x] Swift ラッパー作成 (`SubsetJuliaVMApp/.../UnicodeHelper.swift`)

#### 新規ファイル

| ファイル | 説明 |
|----------|------|
| `subset_julia_vm/src/unicode.rs` | LaTeX→Unicode マッピングテーブル (~300エントリ) |
| `SubsetJuliaVMApp/.../UnicodeHelper.swift` | Swift FFI ラッパー |

#### API

```swift
// Swift
let helper = UnicodeHelper.shared

// 単一変換
helper.lookup("\\alpha")  // → "α"

// 補完候補取得
helper.completions(for: "\\alph")  // → [UnicodeCompletion(latex: "\\alpha", unicode: "α")]

// 文字列内一括展開
helper.expand("x\\^2 + y\\^2")  // → "x² + y²"

// 逆引き
helper.reverseLookup("α")  // → "\\alpha"
```

#### 次のステップ (オプション)

- [ ] iOS アプリの入力 UI に統合 (Tab 補完 or シンボルピッカー)
- [ ] XCFramework 再ビルド (`make xcframework`)

---

## コードエディタ選定

Monaco Editor（WKWebView経由）とネイティブ UITextView 実装の比較検討。

### 要件

1. **Cmd + D でマルチカーソル** - 同じ単語を複数選択して同時編集
2. **コード補完** - キーワード・関数名のオートコンプリート
3. **シンタックスハイライト** - Julia構文の色分け表示

### 要件の実現可能性

| 機能 | UITextView | Monaco/CodeMirror |
|------|------------|-------------------|
| シンタックスハイライト | ✅ 実装済み | ✅ 内蔵 |
| コード補完 | ⚠️ 実装可能（中程度） | ✅ 内蔵 |
| マルチカーソル (Cmd+D) | ❌ **非対応** | ✅ 内蔵 |

### マルチカーソルの技術的制約

UITextViewは`selectedRange`（単一範囲）のみをサポート。マルチカーソル実装には：

1. **カスタムTextKit実装**: テキストレンダリング・カーソル描画・入力処理を全て自前で管理
   - 工数: 非常に大
   - リスク: バグ発生率高、メンテナンス困難

2. **WKWebView + Monaco/CodeMirror**: マルチカーソル機能が内蔵
   - 工数: 中（ブリッジ実装が必要）
   - トレードオフ: パフォーマンス低下、バンドルサイズ増加

### 比較表

| 項目 | ネイティブ (UITextView) | Monaco (WKWebView) |
|------|------------------------|-------------------|
| パフォーマンス | ◎ ネイティブ、高速 | △ WebView経由で遅い |
| バンドルサイズ | ◎ 追加なし | × 数MB追加 |
| シンタックスハイライト | ○ 正規表現ベース | ◎ 完全なLSPサポート可能 |
| 自動補完 | △ 手動実装必要 | ◎ 内蔵 |
| メモリ使用量 | ◎ 軽量 | × 重い |
| キーボード連携 | ◎ 完全 | △ 問題が起きやすい |
| バッテリー効率 | ◎ 良好 | △ WebView常駐で消費大 |
| オフライン対応 | ◎ 完全 | △ バンドル必要 |

### Monaco Editor の課題

#### 技術的課題

1. **WKWebView経由の制約**
   - JavaScriptとSwift間のブリッジ通信が必要
   - 双方向データバインディングの実装が複雑
   - デバッグが困難

2. **iOSキーボードとの統合**
   - ソフトウェアキーボードの表示/非表示検知
   - キーボード上のツールバー（アクセサリビュー）のカスタマイズ困難
   - IME（日本語入力）との相性問題

3. **リソース消費**
   - Monaco本体: 約2-3MB
   - 言語サポート追加で更に増加
   - 初期化時間が長い（コールドスタート問題）

#### UX課題

- スクロールの慣性が WebView と Native で異なる
- テキスト選択のハンドルがネイティブと挙動が違う
- コピー/ペーストメニューのカスタマイズ制限

### 現在の実装: ネイティブ UITextView

#### 採用理由

1. **SubsetJuliaVMの特性に適合**
   - 限定されたJulia構文のみサポート
   - 完全なLSP/言語サーバーは不要
   - 正規表現ベースのハイライトで十分

2. **iOS最適化**
   - ネイティブUIで高速・省電力
   - キーボード連携が完全
   - App Store審査に影響なし

3. **実装済み機能**
   - Monokaiテーマのシンタックスハイライト
   - 行番号表示（LineNumberGutterView）
   - キーワード、文字列、数値、コメント、関数の色分け
   - バックグラウンドスレッドでのハイライト処理

#### 実装ファイル

```
SubsetJuliaVMApp/Views/Editor/
├── MonospacedTextEditor.swift    # メインエディタ実装
│   ├── LineNumberedTextEditor    # 行番号付きエディタ
│   ├── LineNumberedEditorView    # UIView コンテナ
│   ├── LineNumberGutterView      # 行番号描画
│   ├── MonospacedTextEditor      # 基本エディタ
│   ├── JuliaSyntax               # 正規表現パターン
│   └── MonokaiTheme              # カラーテーマ
```

### 代替ライブラリ（将来の検討用）

#### 1. Runestone
- **GitHub**: https://github.com/simonbs/Runestone
- **特徴**: Swift製ネイティブエディタ、tree-sitter対応
- **利点**: 高度なシンタックスハイライト、ネイティブパフォーマンス
- **考慮点**: tree-sitter-juliaの統合が必要

#### 2. CodeMirror 6
- **URL**: https://codemirror.net/6/
- **特徴**: 軽量Web版エディタ
- **利点**: Monacoより軽量、モバイル考慮した設計
- **考慮点**: WKWebView経由の制約は残る

#### 3. カスタムTextKit 2実装
- **特徴**: iOS 15+のTextKit 2を使用
- **利点**: 完全なネイティブ制御
- **考慮点**: 実装コストが高い

### 結論

#### マルチカーソルが必須の場合

**Monaco Editor または CodeMirror 6 の導入を推奨。**

推奨順:
1. **CodeMirror 6** - 軽量、モバイル対応設計、マルチカーソル対応
2. **Monaco Editor** - 機能豊富だがサイズ大

#### マルチカーソルが不要の場合

**現在のネイティブUITextView実装を維持。**

コード補完のみ追加実装:
- `UIMenu` または カスタムポップオーバーで補完候補表示
- SubsetJuliaVMのキーワード・ビルトイン関数リストを使用

---

## 関連ドキュメント

- [OVERVIEW.md](./OVERVIEW.md) - iOS アプリ全体概要
- [STATUS.md](./STATUS.md) - 現在の状況
