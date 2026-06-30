# docs/ 構成

## 基本原則

**サブセット互換性保証**: SubsetJuliaVM で動作するコードは、Julia でも同一の結果を返すことを保証する。

詳細は `docs/vm/DESIGN.md` の「サブセット互換性保証」セクションを参照。

## ディレクトリ構成

- `docs/vm/` : Rust VM / コンパイラ / パーサ / Core IR など実装ドキュメント
- `docs/ios/` : iOS アプリ (SubsetJuliaVMApp) の設計・実装・フェーズ資料
- `docs/web/` : subset_julia_vm_web 関連ドキュメント

## 主要入口

- `docs/vm/DESIGN.md` - **設計思想・サブセット互換性保証**
- `docs/vm/IMPLEMENTATION_PLAN.md`
- `docs/vm/00_STATUS.md`
- `docs/vm/DONE.md`
- `docs/ios/01_OVERVIEW.md`
- `docs/ios/DONE.md`
- `docs/web/SUBSET_JULIA_VM_WEB.md`
