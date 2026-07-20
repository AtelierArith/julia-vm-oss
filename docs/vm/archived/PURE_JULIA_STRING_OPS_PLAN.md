# Pure Julia 文字列操作実装計画

**作成日**: 2026-01-05

## 概要

Phase 2 の文字列操作関数（`strip`, `lstrip`, `rstrip`, `chomp`, `chop`）を Pure Julia で実装する計画。

## 調査結果

### サポート済み機能

| 機能 | 状態 | 例 |
|------|------|-----|
| 文字列イテレーション | ✅ | `for c in s` |
| 文字列連結 `*` | ✅ | `"hello" * " world"` |
| Char → String 追加 | ✅ | `"" * c` |
| `isspace(c)` | ✅ | Pure Julia 実装済み (`strings.jl`) |
| `ncodeunits(s)` | ✅ | Rust builtin |
| `codeunit(s, i)` | ✅ | Rust builtin |
| `Char(n)` | ✅ | Rust builtin |

### 未サポート機能

| 機能 | 状態 | 影響 |
|------|------|------|
| `SubString` | ❌ | 新規文字列を構築する必要あり |
| `pairs(s)` for String | ❌ | インデックス取得に工夫が必要 |
| `string()` 関数 | ❌ | `*` 演算子で代替 |
| 文字列スライス `s[i:j]` | ❌ | 文字単位で再構築が必要 |

## 実装アプローチ

Julia 公式は `SubString` を返すが、SubsetJuliaVM では新規 String を返す。
これは Rust 現行実装と同等の動作。

### 基本パターン

```julia
# 文字列構築パターン
result = ""
for c in s
    if condition(c)
        result = result * c
    end
end
```

## 実装計画

### 1. `lstrip(s)` - 先頭空白削除

```julia
function lstrip(s)
    found_nonspace = false
    result = ""
    for c in s
        if !found_nonspace && isspace(Int(c))
            continue
        end
        found_nonspace = true
        result = result * c
    end
    return result
end
```

### 2. `rstrip(s)` - 末尾空白削除

```julia
function rstrip(s)
    # 末尾から空白でない位置を見つける
    n = ncodeunits(s)
    last_nonspace = 0
    pos = 0
    for c in s
        pos = pos + 1
        if !isspace(Int(c))
            last_nonspace = pos
        end
    end

    # 先頭から last_nonspace まで再構築
    result = ""
    pos = 0
    for c in s
        pos = pos + 1
        if pos > last_nonspace
            break
        end
        result = result * c
    end
    return result
end
```

### 3. `strip(s)` - 両端空白削除

```julia
function strip(s)
    return lstrip(rstrip(s))
end
```

### 4. `chomp(s)` - 末尾改行削除

```julia
function chomp(s)
    n = ncodeunits(s)
    if n == 0
        return s
    end

    # 末尾バイトを確認
    last_byte = codeunit(s, n)

    # \n (10) でない場合はそのまま返す
    if last_byte != 10
        return s
    end

    # \r\n かどうか確認
    remove_count = 1
    if n >= 2 && codeunit(s, n - 1) == 13  # \r = 13
        remove_count = 2
    end

    # 末尾を除いて再構築
    result = ""
    pos = 0
    target_len = n - remove_count
    for c in s
        pos = pos + 1
        if pos > target_len
            break
        end
        result = result * c
    end
    return result
end
```

### 5. `chop(s)` - 末尾文字削除

```julia
function chop(s)
    n = ncodeunits(s)
    if n == 0
        return ""
    end

    # 最後の文字を除いて再構築
    result = ""
    count = 0
    total = 0
    for c in s
        total = total + 1
    end

    pos = 0
    for c in s
        pos = pos + 1
        if pos >= total
            break
        end
        result = result * c
    end
    return result
end
```

## 注意事項

### UTF-8 処理

- `for c in s` は UTF-8 を正しくデコードして文字単位でイテレート
- `ncodeunits(s)` はバイト数を返す（文字数ではない）
- マルチバイト文字を含む文字列でも正しく動作

### パフォーマンス

- 各文字を `*` で連結するのは O(n²) の可能性あり
- Rust builtin は O(n) で効率的
- 短い文字列では実用上問題なし

### 互換性

- Julia 公式は `SubString` を返す（ゼロコピー）
- SubsetJuliaVM は新規 `String` を返す
- 機能的には同等（値が同じ）

## タスクリスト

1. [ ] `strings.jl` に関数を追加
2. [ ] `exports.jl` にエクスポートを追加
3. [ ] Fixture テストを追加
4. [ ] Rust builtin をフォールバックとして維持（オプション）
5. [ ] `STATUS.md`, `DONE.md` を更新

## 将来の改善案

1. **SubString 型の追加**: ゼロコピー文字列ビューを実装
2. **pairs(s) の文字列対応**: インデックス付きイテレーションを追加
3. **string() 関数の有効化**: コンパイラで認識されるように修正
4. **述語関数サポート**: `strip(isspace, s)` パターンを追加

## 参考資料

- Julia 公式実装: `julia/base/strings/util.jl`
- 現在の Rust 実装: `subset_julia_vm_vm/src/vm/builtins_exec.rs`
- Pure Julia 文字列関数: `subset_julia_vm/src/julia/base/strings.jl`
