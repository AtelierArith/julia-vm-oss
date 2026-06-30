# SubsetJuliaVM Project Makefile
# ========================

.PHONY: all clean test test-rust test-ios test-web build-rust build-ios build-wasm help \
	build-aot check-aot \
	test-fixture test-integration test-dispatch test-aot test-parser test-samples \
	test-unicode test-categories test-quick test-panic-free test-exports test-core-ir-aot \
	test-include

# Default target
all: build-rust

# Help
help:
	@echo "SubsetJuliaVM Project Commands"
	@echo "========================="
	@echo ""
	@echo "Build Commands:"
	@echo "  make build-rust     - Build Rust VM (host)"
	@echo "  make build-aot      - Build Rust VM with AoT feature"
	@echo "  make build-ios      - Build iOS app (simulator)"
	@echo "  make build-wasm     - Build WASM module"
	@echo ""
	@echo "Test Commands:"
	@echo "  make test           - Run all tests"
	@echo "  make test-rust      - Run Rust VM tests"
	@echo "  make test-quick     - Run fixture + integration tests only"
	@echo "  make test-fixture   - Run all fixture tests"
	@echo "  make test-fixture C=arithmetic - Run specific fixture category"
	@echo "  make test-integration - Run integration tests only"
	@echo "  make test-dispatch  - Run dispatch tests only"
	@echo "  make test-aot       - Run AOT e2e tests only"
	@echo "  make test-parser    - Run parser tests only"
	@echo "  make test-samples   - Run code samples tests only"
	@echo "  make test-unicode   - Run unicode tests only"
	@echo "  make test-panic-free - Run panic-free VM tests only"
	@echo "  make test-exports  - Run base exports consistency tests only"
	@echo "  make test-core-ir-aot - Run Core IR AOT tests only"
	@echo "  make test-include  - Run include tests only"
	@echo "  make test-categories - List fixture categories with test counts"
	@echo "  make test-ios       - Run iOS app tests (CLI)"
	@echo "  make test-ios-samples - Run iOS sample code tests only"
	@echo "  make test-web       - Run web tests (requires server)"
	@echo ""
	@echo "Clean Commands:"
	@echo "  make clean          - Clean all build artifacts"
	@echo "  make clean-rust     - Clean Rust build"
	@echo "  make clean-ios      - Clean iOS build"

# ========================
# Build Targets
# ========================

build-rust:
	cd subset_julia_vm && cargo build --release

build-ios:
	xcodebuild \
		-project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
		-scheme SubsetJuliaVMApp \
		-sdk iphonesimulator \
		-destination 'platform=iOS Simulator,name=iPad (A16)' \
		build

build-aot:
	cargo build --features aot

build-wasm:
	cd subset_julia_vm_web && wasm-pack build --target web --profile web-release

# ========================
# Test Targets
# ========================

# Run all tests
test: test-rust test-ios

# Run Rust tests
test-rust:
	cd subset_julia_vm && cargo test

# Run fixture tests (optionally filtered by category: make test-fixture C=arithmetic)
test-fixture:
ifdef C
	timeout 300 cargo test --test fixture_tests $(C)
else
	timeout 300 cargo test --test fixture_tests
endif

# Run integration tests only
test-integration:
	timeout 300 cargo test --test integration_tests

# Run dispatch tests only
test-dispatch:
	timeout 300 cargo test --test dispatch_tests

# Run AOT e2e tests only
test-aot:
	timeout 300 cargo test --test aot_e2e_tests

# Run parser tests only
test-parser:
	timeout 300 cargo test --test parser_pure_rust

# Run code samples tests only
test-samples:
	timeout 300 cargo test --test code_samples_tests

# Run unicode tests only
test-unicode:
	timeout 300 cargo test --test unicode_tests

# Run panic-free VM tests only
test-panic-free:
	timeout 300 cargo test --test panic_free_vm_tests

# Run base exports consistency tests only
test-exports:
	timeout 300 cargo test --test base_exports_consistency_tests

# Run Core IR AOT tests only
test-core-ir-aot:
	timeout 300 cargo test --test core_ir_aot_tests

# Run include tests only
test-include:
	timeout 300 cargo test --test include_tests

# Run fixture + integration tests (most common daily use)
test-quick:
	timeout 300 cargo test --test fixture_tests --test integration_tests

# List fixture categories with test counts
test-categories:
	@echo "Fixture Test Categories:"
	@echo "========================"
	@for dir in subset_julia_vm/tests/fixtures/*/; do \
		category=$$(basename "$$dir"); \
		count=$$(grep -c '^\[\[tests\]\]' "$$dir/manifest.toml" 2>/dev/null || echo 0); \
		printf "  %-25s %3d tests\n" "$$category" "$$count"; \
	done
	@echo ""
	@total=$$(grep -rh '^\[\[tests\]\]' subset_julia_vm/tests/fixtures/*/manifest.toml 2>/dev/null | wc -l | tr -d ' '); \
	echo "Total: $$total tests across $$(ls -d subset_julia_vm/tests/fixtures/*/ | wc -l | tr -d ' ') categories"

# Run all iOS tests
test-ios:
	xcodebuild test \
		-project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
		-scheme SubsetJuliaVMApp \
		-sdk iphonesimulator \
		-destination 'platform=iOS Simulator,name=iPad (A16)' \
		-resultBundlePath TestResults.xcresult \
		| xcpretty --color || true
	@echo ""
	@echo "Test results saved to TestResults.xcresult"

# Run iOS sample code tests only
test-ios-samples:
	xcodebuild test \
		-project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
		-scheme SubsetJuliaVMApp \
		-sdk iphonesimulator \
		-destination 'platform=iOS Simulator,name=iPad (A16)' \
		-only-testing:SubsetJuliaVMAppTests/SampleCodeTests \
		| xcpretty --color || true

# Run iOS sample all-in-one test
test-ios-all-samples:
	xcodebuild test \
		-project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
		-scheme SubsetJuliaVMApp \
		-sdk iphonesimulator \
		-destination 'platform=iOS Simulator,name=iPad (A16)' \
		-only-testing:SubsetJuliaVMAppTests/SampleCodeTests/testAllSamples \
		| xcpretty --color || true

# Run web tests (requires server running on port 8080)
test-web:
	cd web && npm test

# ========================
# Clean Targets
# ========================

clean: clean-rust clean-ios

clean-rust:
	cd subset_julia_vm && cargo clean
	cd subset_julia_vm_web && cargo clean

clean-ios:
	xcodebuild clean \
		-project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
		-scheme SubsetJuliaVMApp
	rm -rf TestResults.xcresult

# ========================
# Utility Targets
# ========================

# Start web server for testing
serve-web:
	cd web && python3 -m http.server 8080

# Format Rust code
fmt:
	cd subset_julia_vm && cargo fmt
	cd subset_julia_vm_web && cargo fmt

# Check Rust code
check:
	cd subset_julia_vm && cargo check
	cd subset_julia_vm_web && cargo check

# Check Rust code with AoT feature (prevents feature-gated breakage)
check-aot:
	cargo check --features aot
