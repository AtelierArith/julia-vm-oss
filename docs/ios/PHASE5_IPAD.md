# Phase 5: iPad Optimization & Polish

**Goal**: iPad 大画面活用、仕上げ

**Status**: ❌ **未着手**

## Task 5.1: iPad Layout (Split View)

```swift
struct ContentView: View {
    @Environment(\.horizontalSizeClass) var sizeClass

    var body: some View {
        if sizeClass == .regular {
            // iPad: Split view
            NavigationSplitView {
                Sidebar()
            } detail: {
                EditorDetailView()
            }
        } else {
            // iPhone: Tabbed interface
            TabView {
                EditorView()
                    .tabItem { Label("Editor", systemImage: "doc.text") }

                SampleLibraryView()
                    .tabItem { Label("Examples", systemImage: "book") }

                MyScriptsView()
                    .tabItem { Label("My Scripts", systemImage: "folder") }
            }
        }
    }
}
```

**Checklist**:
- [ ] Implement split view for iPad
- [ ] Test on iPad simulator
- [ ] Add keyboard shortcuts
- [ ] Support external keyboard
- [ ] Test multi-window (iPadOS 16+)

## Task 5.2: Keyboard Shortcuts

```swift
extension View {
    func editorKeyboardShortcuts() -> some View {
        self
            .keyboardShortcut("r", modifiers: .command) // Run
            .keyboardShortcut("k", modifiers: .command) // Clear
            .keyboardShortcut(",", modifiers: .command) // Settings
            .keyboardShortcut("n", modifiers: .command) // New script
    }
}
```

**Checklist**:
- [ ] Add keyboard shortcuts
- [ ] Document shortcuts in help
- [ ] Test on iPad with keyboard
- [ ] Add shortcut hints in UI

## Task 5.3: Accessibility

```swift
// Accessibility labels
Text("Run code")
    .accessibility(label: Text("Run Julia code"))
    .accessibility(hint: Text("Executes the current code in the editor"))

// Dynamic Type
Text(output)
    .font(.system(.body, design: .monospaced))
    .dynamicTypeSize(...DynamicTypeSize.xxxLarge)
```

**Checklist**:
- [ ] Add accessibility labels to all interactive elements
- [ ] Test with VoiceOver
- [ ] Support Dynamic Type
- [ ] Test with high contrast
- [ ] Support Reduce Motion

## Task 5.4: Animations & Transitions

```swift
// Error banner slide-in
.transition(.move(edge: .top).combined(with: .opacity))
.animation(.spring(response: 0.3), value: error)

// Button press feedback
.buttonStyle(.borderedProminent)
.hoverEffect(.lift) // iPadOS

// Loading states
if isRunning {
    ProgressView()
        .transition(.opacity)
}
```

**Checklist**:
- [ ] Add smooth transitions
- [ ] Add button feedback
- [ ] Add loading states
- [ ] Test performance with animations
- [ ] Support Reduce Motion

## Acceptance Criteria

- [ ] iPad split view works correctly
- [ ] Keyboard shortcuts functional
- [ ] VoiceOver reads all content correctly
- [ ] Dynamic Type supported
- [ ] Animations smooth (60fps)
- [ ] Multi-window support (iPad)
