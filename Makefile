CARGO ?= cargo

# Installation location (macOS). Override with e.g. `make install BINDIR=/usr/local/bin`.
# ~/.local/bin is user-writable and needs no sudo; add it to PATH if it isn't already.
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
BIN_NAME := rehab
RELEASE_BIN := target/release/$(BIN_NAME)

.PHONY: help all lint fmt fmt-check clippy compile build release test check clean install uninstall

help: ## Display available targets
	@awk 'BEGIN {FS = ":.*## "}; /^[a-zA-Z0-9_-]+:.*## / {printf "\033[36m%-28s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

all: lint compile test ## Run lint, compile, and test

lint: fmt-check clippy ## Check formatting and run the linter

fmt: ## Format the source code in place
	$(CARGO) fmt --all

fmt-check: ## Verify formatting without modifying files
	$(CARGO) fmt --all -- --check

clippy: ## Run Clippy, treating warnings as errors
	$(CARGO) clippy --all-targets --all-features -- -D warnings

compile: ## Compile the application
	$(CARGO) build

build: compile ## Alias for compile

release: ## Compile with optimizations
	$(CARGO) build --release

test: ## Run the test suite
	$(CARGO) test --all-targets

check: ## Type-check without producing binaries
	$(CARGO) check --all-targets

clean: ## Remove build artifacts
	$(CARGO) clean

install: release ## Install the release binary into BINDIR (default: ~/.local/bin)
	@mkdir -p "$(BINDIR)"
	install -m 0755 "$(RELEASE_BIN)" "$(BINDIR)/$(BIN_NAME)"
	@echo "Installed $(BIN_NAME) to $(BINDIR)/$(BIN_NAME)"
	@case ":$$PATH:" in \
		*":$(BINDIR):"*) ;; \
		*) echo "note: $(BINDIR) is not on your PATH; add it, e.g.:"; \
		   echo "      echo 'export PATH=\"$(BINDIR):\$$PATH\"' >> ~/.zshrc" ;; \
	esac

uninstall: ## Remove the installed binary from BINDIR
	rm -f "$(BINDIR)/$(BIN_NAME)"
	@echo "Removed $(BINDIR)/$(BIN_NAME)"
