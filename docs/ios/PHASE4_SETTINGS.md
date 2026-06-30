# Phase 4: Settings & Configuration

**Goal**: エディタと VM のユーザー設定

**Status**: ❌ **未着手**

## Task 4.1: Settings Model

**File**: `Models/Settings.swift`

```swift
class Settings: ObservableObject {
    // Editor
    @AppStorage("editorFontSize") var editorFontSize: Double = 14
    @AppStorage("showLineNumbers") var showLineNumbers = true
    @AppStorage("tabWidth") var tabWidth = 4

    // Appearance
    @AppStorage("theme") var theme: Theme = .system

    // VM Limits
    @AppStorage("maxSteps") var maxSteps: UInt64 = 1_000_000
    @AppStorage("maxLoopIterations") var maxLoopIterations: UInt64 = 10_000_000
    @AppStorage("executionTimeout") var executionTimeout: Double = 30.0

    enum Theme: String, CaseIterable {
        case light = "Light"
        case dark = "Dark"
        case system = "System"

        var colorScheme: ColorScheme? {
            switch self {
            case .light: return .light
            case .dark: return .dark
            case .system: return nil
            }
        }
    }
}
```

**Checklist**:
- [ ] Define settings model
- [ ] Use @AppStorage for persistence
- [ ] Add validation for numeric values
- [ ] Document each setting

## Task 4.2: Settings UI

**File**: `Views/Settings/SettingsView.swift`

```swift
struct SettingsView: View {
    @StateObject private var settings = Settings()

    var body: some View {
        NavigationView {
            Form {
                // Editor Section
                Section("Editor") {
                    // Font size stepper
                    // Line numbers toggle
                    // Tab width picker
                }

                // Appearance Section
                Section("Appearance") {
                    // Theme picker (Light/Dark/System)
                }

                // Execution Limits Section
                Section("Execution Limits") {
                    // Max instructions slider
                    // Max loop iterations slider
                    // Timeout slider
                }

                // About Section
                Section("About") {
                    // Version
                    // Documentation link
                    // Report issue link
                }
            }
        }
    }
}
```

**Checklist**:
- [ ] Create settings view
- [ ] Add all settings controls
- [ ] Test persistence
- [ ] Add validation feedback
- [ ] Link to documentation

## Acceptance Criteria

- [ ] All settings persist across app restarts
- [ ] Font size updates editor immediately
- [ ] Theme changes apply correctly
- [ ] VM limits are sent to Rust VM
- [ ] Settings UI is intuitive
- [ ] Default values match AppConfiguration
