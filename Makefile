SHELL := /usr/bin/env bash

WEB_DIST := crates/miku-server/web-dist
WEB_WASM := target/wasm32-unknown-unknown/release/miku_web.wasm
WASM_BINDGEN_VERSION := 0.2.121

.PHONY: build-web-assets check-web fmt clippy test check install-wasm-bindgen clean-web-assets

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
