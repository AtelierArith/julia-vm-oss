# Phase 1（Foundation）メモ: 取り扱いについて

**最終更新**: 2026-01-17

このファイルは 2026-01-13 時点の “Phase 1 計画（型推論強化 + AoT 基盤構築）” をまとめたものでしたが、現在のコードベースでは:

- `subset_julia_vm_runtime` クレートが既に存在
- AoT CLI（`subset_julia_vm/src/bin/aot.rs`）が存在し、DCE/推論/最適化/コード生成が接続済み
- 計画文書の内容と、実際の構造体・モジュール・フローがズレ始めた

ため、**歴史的資料としての役割に切り替え**、詳細は現行ドキュメントへ集約します。

## 現行の参照先

- `docs/aot/README.md`: 使い方（CLI/オプション/リンク）
- `docs/aot/DESIGN.md`: 現行実装ベースの設計メモ
- `docs/aot/IMPLEMENTATION_GUIDE.md`: どこを触るか（開発者向け）
- `docs/aot/IMPLEMENTATION_PLAN.md`: 現状ベースのロードマップ

