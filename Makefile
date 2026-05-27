SHELL := /usr/bin/env bash

APP_NAME := Miku
MACOS_TARGET := aarch64-apple-darwin
VERSION := $(shell awk -F' *= *' '/^version = / {gsub(/"/,"",$$2); print $$2; exit}' Cargo.toml)
DIST_DIR := dist/macos
APP_DIR := $(DIST_DIR)/$(APP_NAME).app
DMG_NAME := $(APP_NAME)-$(VERSION)-$(MACOS_TARGET).dmg
DMG_PATH := $(DIST_DIR)/$(DMG_NAME)

WEB_DIST := crates/miku-server/web-dist
WEB_WASM := target/wasm32-unknown-unknown/release/miku_web.wasm
WASM_BINDGEN_VERSION := $(shell awk -F' *= *' '/^wasm-bindgen = / {gsub(/"/,"",$$2); print $$2; exit}' Cargo.toml)

.PHONY: build-macos-app package-macos-dmg clean-macos-artifacts build-web-assets check-web fmt clippy test check install-wasm-bindgen clean-web-assets

build-macos-app:
	cargo build --release -p miku-cli --target $(MACOS_TARGET)
	./scripts/macos/build_app.sh \
		--target $(MACOS_TARGET) \
		--version $(VERSION) \
		--output-dir $(DIST_DIR)

package-macos-dmg: build-macos-app
	./scripts/macos/package_dmg.sh \
		--app-path $(APP_DIR) \
		--output-path $(DMG_PATH)

clean-macos-artifacts:
	rm -rf $(DIST_DIR)

build-web-assets:
	cargo build -p miku-web --release --target wasm32-unknown-unknown
	mkdir -p $(WEB_DIST)
	find $(WEB_DIST) -mindepth 1 ! -name ".gitkeep" -delete
	wasm-bindgen --target web --out-dir $(WEB_DIST) --out-name miku_web $(WEB_WASM)
	cp crates/miku-web/index.html $(WEB_DIST)/index.html

check-web:
	cargo build -p miku-web --target wasm32-unknown-unknown

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

check: fmt clippy build-web-assets test check-web

install-wasm-bindgen:
	cargo install wasm-bindgen-cli --version $(WASM_BINDGEN_VERSION) --locked

clean-web-assets:
	mkdir -p $(WEB_DIST)
	find $(WEB_DIST) -mindepth 1 ! -name ".gitkeep" -delete
