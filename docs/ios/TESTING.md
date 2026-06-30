# iOS アプリテスト戦略

## テスト実行方法

### コマンドラインから実行

```bash
# シミュレータのUDIDを確認
xcrun simctl list devices available

# テスト実行（iPad シミュレータの例）
xcodebuild \
  -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
  -scheme SubsetJuliaVMAppTests \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPad (A16)' \
  test
```

### Xcodeから実行

1. Xcode でプロジェクトを開く
2. `Cmd + U` でテストを実行
3. または Test Navigator (`Cmd + 6`) から個別テストを実行

## テストファイル構成

```
SubsetJuliaVMApp/SubsetJuliaVMAppTests/
├── Models/
│   ├── CodeSampleTests.swift
│   ├── VMErrorTests.swift
│   └── ExecutionResultTests.swift
├── Services/
│   ├── VMBridgeTests.swift
│   └── ScriptStoreTests.swift
├── Views/
│   └── ContentViewTests.swift
└── Integration/
    └── SampleExecutionTests.swift
```

## テストカテゴリ

### Unit Tests

```swift
// Models
class CodeSampleTests: XCTestCase {
    func testCategoryCount() {
        XCTAssertEqual(CodeSample.Category.allCases.count, 8)
    }

    func testSamplesLoaded() {
        XCTAssertGreaterThanOrEqual(CodeSample.samples.count, 47)
    }
}

// Services
class VMBridgeTests: XCTestCase {
    func testBasicExecution() {
        let result = "1 + 2".withCString { compile_and_run($0, 42) }
        XCTAssertEqual(result, 3.0)
    }
}
```

### Integration Tests

```swift
class SampleExecutionTests: XCTestCase {
    func testAllSamplesExecute() {
        for sample in CodeSample.samples {
            let result = sample.code.withCString { compile_and_run($0, 42) }
            XCTAssertFalse(result.isNaN, "Sample '\(sample.name)' failed")
        }
    }
}
```

### UI Tests

```swift
class SubsetJuliaVMAppUITests: XCTestCase {
    func testSampleSelection() {
        let app = XCUIApplication()
        app.launch()

        // Select a sample
        app.buttons["Hello World"].tap()

        // Run code
        app.buttons["Run"].tap()

        // Verify output
        XCTAssertTrue(app.staticTexts["Hello, World!"].exists)
    }
}
```

## 手動テストチェックリスト

### 基本機能

- [ ] アプリ起動
- [ ] サンプル選択
- [ ] コード実行
- [ ] 出力表示
- [ ] IR 表示

### エディタ

- [ ] コード入力
- [ ] 日本語入力
- [ ] コピー&ペースト
- [ ] スクロール

### エラー表示

- [ ] 構文エラー表示
- [ ] 未対応機能エラー
- [ ] ランタイムエラー

### iPad

- [ ] Split View
- [ ] 外部キーボード
- [ ] マルチウィンドウ

## パフォーマンス指標

| メトリック | 目標 |
|-----------|------|
| アプリ起動 | < 1秒 |
| サンプル読み込み | 即時 |
| コード実行 | 通常 < 100ms |
| UI レスポンス | 60fps |
| メモリ使用量 | < 50MB |
