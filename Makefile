BINARY := lora-chat
TARGET_DIR := target
RELEASE_BIN := $(TARGET_DIR)/release/$(BINARY)
DEBUG_BIN := $(TARGET_DIR)/debug/$(BINARY)

.PHONY: all build release run run-mock clean test clippy fmt scan diagnostics help

all: build

build: ## debug build
	cargo build

release: ## optimized release build
	cargo build --release

run: build $(DEBUG_BIN) ## run debug TUI (auto-connects)
	$(DEBUG_BIN)

run-mock: build $(DEBUG_BIN) ## run TUI with simulated device
	$(DEBUG_BIN) --mock

scan: $(DEBUG_BIN) ## scan for serial devices and identify firmware
	$(DEBUG_BIN) --scan

diagnostics: $(DEBUG_BIN) ## run diagnostics against detected device
	$(DEBUG_BIN) --diagnostics

test: ## run test suite
	cargo test

clippy: ## lint
	cargo clippy --all-targets

fmt: ## format code
	cargo fmt

clean: ## remove build artifacts and app data cache
	cargo clean
	rm -rf $(TARGET_DIR)

help: ## show this help
	@grep -E '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*## "} {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'
