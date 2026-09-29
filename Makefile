CARGO ?= cargo

# Installation location (macOS). Override with e.g. `make install BINDIR=/usr/local/bin`.
# ~/.local/bin is user-writable and needs no sudo; add it to PATH if needed.
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
MANDIR ?= $(PREFIX)/share/man
MAN1DIR := $(MANDIR)/man1
MAN5DIR := $(MANDIR)/man5
BIN_NAME := rehab
RELEASE_BIN := target/release/$(BIN_NAME)
MAN1_PAGE := man/rehab.1
MAN5_PAGE := man/rehab-config.5

.PHONY: help all lint fmt fmt-check clippy compile build release test check clean install uninstall man install-man uninstall-man

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

man: ## Generate man/rehab.1 from the CLI definition (rehab-config.5 is hand-written)
	$(CARGO) run --quiet --bin gen-man -- man
	@echo "Generated $(MAN1_PAGE) (hand-written: $(MAN5_PAGE))"

test: ## Run the test suite
	$(CARGO) test --all-targets

check: ## Type-check without producing binaries
	$(CARGO) check --all-targets

clean: ## Remove build artifacts
	$(CARGO) clean

install: release install-man ## Install the release binary and man pages (BINDIR default: ~/.local/bin)
	@mkdir -p "$(BINDIR)"
	install -m 0755 "$(RELEASE_BIN)" "$(BINDIR)/$(BIN_NAME)"
	@echo "Installed $(BIN_NAME) to $(BINDIR)/$(BIN_NAME)"
	@case ":$$PATH:" in \
		*":$(BINDIR):"*) ;; \
		*) echo "note: $(BINDIR) is not on your PATH; add it, e.g.:"; \
		   echo "      echo 'export PATH=\"$(BINDIR):\$$PATH\"' >> ~/.zshrc" ;; \
	esac

install-man: man ## Install man pages into MANDIR (default: ~/.local/share/man)
	@mkdir -p "$(MAN1DIR)" "$(MAN5DIR)"
	install -m 0644 "$(MAN1_PAGE)" "$(MAN1DIR)/rehab.1"
	install -m 0644 "$(MAN5_PAGE)" "$(MAN5DIR)/rehab-config.5"
	@echo "Installed man pages to $(MAN1DIR)/rehab.1 and $(MAN5DIR)/rehab-config.5"
	@case ":$$MANPATH:" in \
		*":$(MANDIR):"*) ;; \
		*) echo "note: view with 'man rehab' and 'man rehab-config'."; \
		   echo "      if not found, ensure $(MANDIR) is on your MANPATH." ;; \
	esac

uninstall: uninstall-man ## Remove the installed binary and man pages
	rm -f "$(BINDIR)/$(BIN_NAME)"
	@echo "Removed $(BINDIR)/$(BIN_NAME)"

uninstall-man: ## Remove the installed man pages
	rm -f "$(MAN1DIR)/rehab.1" "$(MAN5DIR)/rehab-config.5"
	@echo "Removed man pages from $(MAN1DIR) and $(MAN5DIR)"
