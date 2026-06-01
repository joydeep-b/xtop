# xtop Makefile - thin wrapper around cargo.

CARGO   ?= cargo
PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
BIN     := xtop

.DEFAULT_GOAL := build

.PHONY: all build release run probe check fmt fmt-check clippy test \
        clean distclean install uninstall help

## all: format check + lint + release build
all: fmt-check clippy release

## build: debug build
build:
	$(CARGO) build

## release: optimized release build
release:
	$(CARGO) build --release

## run: build and run the TUI (debug)
run:
	$(CARGO) run

## probe: one-shot text metrics dump (headless, no TTY needed)
probe:
	$(CARGO) run --release -- --probe

## check: fast type-check without producing a binary
check:
	$(CARGO) check

## fmt: format the code in place
fmt:
	$(CARGO) fmt

## fmt-check: verify formatting (CI-friendly, non-mutating)
fmt-check:
	$(CARGO) fmt --check

## clippy: lint with warnings denied
clippy:
	$(CARGO) clippy --all-targets -- -D warnings

## test: run the test suite
test:
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
clean:
	$(CARGO) clean

## distclean: clean + remove the lockfile
distclean: clean
	rm -f Cargo.lock

## help: list available targets
help:
	@echo "xtop make targets:"
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/^## /  /'
