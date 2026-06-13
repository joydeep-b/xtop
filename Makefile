# xtop Makefile - thin wrapper around cargo.

CARGO   ?= cargo
PREFIX  ?= $(HOME)/.local
BINDIR  ?= $(PREFIX)/bin
BIN     := xtop

.DEFAULT_GOAL := build

.PHONY: all check-cargo build release run probe check fmt fmt-check clippy test \
        clean distclean install uninstall help

## all: format check + lint + release build
all: fmt-check clippy release

check-cargo:
	@command -v "$(CARGO)" >/dev/null 2>&1 || { \
		echo "Error: cargo is required but was not found."; \
		echo "Install Rust and Cargo from https://rust-lang.org/tools/install/"; \
		exit 1; \
	}

## build: debug build
build: check-cargo
	$(CARGO) build

## release: optimized release build
release: check-cargo
	$(CARGO) build --release

## run: build and run the TUI (debug)
run: check-cargo
	$(CARGO) run

## probe: one-shot text metrics dump (headless, no TTY needed)
probe: check-cargo
	$(CARGO) run --release -- --probe

## check: fast type-check without producing a binary
check: check-cargo
	$(CARGO) check

## fmt: format the code in place
fmt: check-cargo
	$(CARGO) fmt

## fmt-check: verify formatting (CI-friendly, non-mutating)
fmt-check: check-cargo
	$(CARGO) fmt --check

## clippy: lint with warnings denied
clippy: check-cargo
	$(CARGO) clippy --all-targets -- -D warnings

## test: run the test suite
test: check-cargo
	$(CARGO) test

## install: install the release binary into $(BINDIR)
install: release
	install -d $(DESTDIR)$(BINDIR)
	install -m 0755 "$$($(CARGO) metadata --format-version 1 --no-deps \
		| sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')/release/$(BIN)" \
		$(DESTDIR)$(BINDIR)/$(BIN)

## uninstall: remove the installed binary
uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(BIN)

## clean: remove build artifacts
clean: check-cargo
	$(CARGO) clean

## distclean: clean + remove the lockfile
distclean: clean
	rm -f Cargo.lock

## help: list available targets
help:
	@echo "xtop make targets:"
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/^## /  /'
