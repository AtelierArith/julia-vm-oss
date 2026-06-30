# Phase 3: Code Persistence & Management

**Goal**: ユーザースクリプトの保存/読み込み、自動保存、サンプルライブラリ

**Status**: ❌ **未着手**

## Task 3.1: Script Storage Service

**File**: `Services/Persistence/ScriptStore.swift`

```swift
class ScriptStore: ObservableObject {
    @Published var userScripts: [UserScript] = []

    private let fileManager = FileManager.default
    private var scriptsDirectory: URL {
        let docs = fileManager.urls(for: .documentDirectory, in: .userDomainMask)[0]
        return docs.appendingPathComponent("UserScripts", isDirectory: true)
    }

    init() {
        createDirectoryIfNeeded()
        loadScripts()
    }

    func loadScripts() { ... }
    func save(_ script: UserScript) { ... }
    func delete(_ script: UserScript) { ... }
    func create(name: String, code: String) -> UserScript { ... }
}
```

**Checklist**:
- [ ] Implement script storage
- [ ] Test save/load
- [ ] Handle file errors gracefully
- [ ] Add auto-save timer (5 seconds after edit)
- [ ] Limit max scripts (50)

## Task 3.2: Sample Library UI

**File**: `Views/Samples/SampleLibraryView.swift`

### 機能

- カテゴリフィルタリング
- 検索機能
- サンプル詳細表示
- 難易度バッジ

```swift
struct SampleLibraryView: View {
    let samples: [CodeSample]
    let onSelect: (CodeSample) -> Void
    @State private var selectedCategory: CodeSample.Category?
    @State private var searchText = ""

    var body: some View {
        NavigationView {
            VStack {
                // Category Filter (horizontal scroll)
                // Sample List (searchable)
            }
        }
    }
}
```

**コンポーネント**:
- `SampleRow` - サンプル行表示
- `FilterChip` - カテゴリフィルタチップ
- `DifficultyBadge` - 難易度バッジ

**Checklist**:
- [ ] Implement sample library UI
- [ ] Add category filtering
- [ ] Add search functionality
- [ ] Create sample row component
- [ ] Test with 38 samples

## Task 3.3: My Scripts UI

**File**: `Views/MyScripts/MyScriptsView.swift`

```swift
struct MyScriptsView: View {
    @StateObject private var store = ScriptStore()
    @State private var showingNewScriptSheet = false
    let onSelect: (UserScript) -> Void

    var body: some View {
        NavigationView {
            List {
                ForEach(store.userScripts) { script in
                    ScriptRow(script: script)
                        .swipeActions { ... }
                }
            }
        }
    }
}
```

**機能**:
- スクリプト一覧
- 新規作成
- 削除（スワイプ）
- お気に入り

**Checklist**:
- [ ] Implement my scripts view
- [ ] Add create/delete functionality
- [ ] Add favorite toggle
- [ ] Add swipe actions
- [ ] Test persistence

## Acceptance Criteria

- [ ] Scripts save to disk correctly
- [ ] Scripts load on app launch
- [ ] Auto-save works (5 seconds after edit)
- [ ] Sample library shows 38 examples
- [ ] Category filtering works
- [ ] Search works
- [ ] My Scripts CRUD operations work
- [ ] Favorite marking persists
