# noteboard development tasks

# Build
build:
    cargo build

# Run tests (optionally pass test name: just test my_test)
test *args:
    cargo test {{args}}

# Run clippy lints (must pass with zero warnings)
lint:
    cargo clippy --all-targets -- -D warnings

# Format code
fmt:
    cargo fmt

# Run all checks (mirrors CI: format + lint + test)
check:
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Install locally as `noteboard`
install:
    cargo install --path crates/noteboard

# Open the TUI on a throwaway sample board
board:
    #!/usr/bin/env bash
    set -e
    cargo build
    dir=$(mktemp -d)
    nb=target/debug/noteboard
    $nb -C "$dir" init --title Sample --columns "Backlog,Todo,In Progress,Done"
    $nb -C "$dir" add "Try dragging me" -s todo -t demo >/dev/null
    $nb -C "$dir" add "Right-click me" -s todo -p high -d +2d >/dev/null
    $nb -C "$dir" add "Double-click to open in your editor" -s in-progress >/dev/null
    $nb "$dir"

# Write the Homebrew formula for a published release into ~/Developer/homebrew-tap
brew-formula version:
    scripts/update-homebrew.sh {{version}}

# Preview the man page
manpage:
    cargo run -- manpage | man -l -

# Clean build artifacts
clean:
    cargo clean
