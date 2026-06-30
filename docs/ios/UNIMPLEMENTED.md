# iOS 未実装機能一覧

**最終更新**: 2026-01-07

> 実装済みの機能は [STATUS.md](STATUS.md) と [COMPLETED_PHASES.md](COMPLETED_PHASES.md) を参照してください。

---

## 未実装（コード上まだ存在しないもの）

### Phase 3: Code Persistence

- `Services/Persistence/` が存在せず、`UserScript` の保存/読み込み/削除/自動保存が未実装
- `Views/MyScripts/` が存在せず、マイスクリプト一覧 UI が未実装
- `Views/Samples/` が存在せず、サンプルライブラリ UI（検索/カテゴリ/詳細表示）が未実装
- 起動時の編集内容・REPL 履歴の永続化は未実装（メモリのみ）

### Phase 4: Settings

- `Models/Settings.swift` が存在せず、`@AppStorage` を使った設定永続化が未実装
- `Views/Settings/SettingsView.swift` が存在せず、設定 UI が未実装
- `AppConfiguration` の値（フォントサイズ/実行制限/テーマ等）は固定値のまま

### Phase 5: iPad Optimization

- `NavigationSplitView` 等の iPad 向けレイアウトが未実装（`ContentView` は単一レイアウト）
- iPad 向けの複数画面/サイドバー構成は未実装

### Phase 6: Advanced Features (Optional)

- 実行履歴モデル/ビュー未実装（`ExecutionHistory`/History View なし）
- 共有/エクスポート機能未実装（ShareSheet や .jl 出力なし）
- コードテンプレート/スニペット挿入 UI 未実装

---

## 実装済みだが検証未実施

- ErrorView/ErrorBanner の UI 実機検証（`ContentView` に統合済みだが検証記録なし）
