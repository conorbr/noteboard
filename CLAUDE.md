# Working on noteboard

A terminal kanban board whose only storage is a folder of Markdown files, plus a CLI
that can do everything the board can (with `--json` for scripts and agents).

## Commands

```sh
just check      # what CI runs: fmt --check, clippy -D warnings, tests
just test NAME  # one test
just board      # build and open the TUI on a throwaway sample board
just install    # cargo install the `noteboard` binary
```

Without `just`: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test`.
Optional pre-commit hook: `git config core.hooksPath .githooks`.

## Layout

Everything is in `crates/noteboard/src`.

**Files and data** (no TUI dependencies)
- `store.rs`: the folder. Loads tickets, creates/updates/duplicates them, archives to
  `archive/`, deletes to `.trash/`, resolves ids, reads README config, `signature()`
  for change detection. Every write re-reads the file first and is atomic.
- `frontmatter.rs`: `Document` edits one key's lines and leaves everything else
  byte-for-byte; values YAML would misread are quoted.
- `settings.rs`: editing README settings (`Settings`, `ColumnSpec`); renaming a column
  rewrites its tickets' `status`.
- `ordering.rs`: which `order:` numbers to write when a card moves (gaps of 10;
  renumber the column only when needed).
- `model.rs` (Task, Priority, BoardConfig), `dates.rs` (`fri`, `+3d`...), `editor.rs`.
- `cli.rs`: clap definitions and every subcommand.
- `keys.rs`: shortcut table (`ACTIONS`), chord parsing, `keys.toml`.
- `theme.rs`: the `palette!` table generates the theme struct, defaults and `theme.toml`.

**TUI**
- `app/`: state and behaviour. `mod.rs` (App, reload, filters, sorting, actions,
  layout, hit map), `input.rs` (keys per mode), `mouse.rs` (clicks and drags),
  `form.rs` (ticket form), `board_settings.rs`, `options.rs`, `menu.rs` (context and
  sort menus), `reorder.rs` (manual order), `text.rs` (`TextField`, wrapping).
- `ui/`: drawing only. `board.rs` owns card geometry (`card_height`), `input.rs`
  draws a `TextField`, `mod.rs` has the bars and `open_popup`.
- `main.rs`: arguments, terminal setup, the event loop, suspending for `$EDITOR`.

## How things fit together

- **Files are the truth.** Change tickets through `Store`, then `app.reload()`.
  Never keep ticket state anywhere else.
- **Minimal writes.** `TaskPatch` fields left `None` are not touched; the form only
  fills in what changed.
- **The hit map.** Each frame starts empty; anything clickable calls
  `app.add_hit(rect, Hit::...)` as it's drawn, and the mouse code asks
  `app.hit_at(x, y)` (last drawn wins). Popups register `Hit::Popup` through
  `ui::open_popup`, so clicks inside don't count as outside. New clickable UI adds a
  hit; it never recomputes layout in the mouse code.
- **One text type.** Every editable string is a `TextField` (`app/text.rs`) and is
  drawn with `ui::input::text_box`. Multiline fields report their layout width back
  (`form.notes_width`) so ↑/↓ match the screen.
- **Shortcuts come from the key map.** Use `app.keys.lookup(scope, &key)` for
  anything rebindable and show keys with `app.keys` rather than hard-coded strings.
- **Colours come from the theme.** Add a colour by adding a line to `palette!`.
- Tickets are identified by path in the TUI and by id (file stem) in the CLI.
- Columns are indexes into `store.config.columns`; per-column vectors are resized on reload.

## Rules

- No `unwrap()` outside tests.
- Every board operation gets a CLI command with `--json`.
- Errors from the CLI go to stderr as `noteboard: error: ...` with exit code 1.
- After a user-visible change, update `README.md` and `CHANGELOG.md`.

## Things that surprise people

- A mouse-up runs `drop_on()` after every click; it only moves a card if a drag
  happened, and dropping a card where it already is does nothing.
- Reordering outside the Manual sort numbers the column in its on-screen order and
  saves `sort: manual` to README.md.
- Tickets without a `status` sit in the first column, so reordering columns moves them.
- Unknown `status` values stay as written until the ticket is moved.
- `-C DIR` / `$NOTEBOARD_DIR` pick the board for subcommands; `noteboard DIR` opens the TUI.

## Releasing

1. Bump `version` in the root `Cargo.toml`, date the CHANGELOG section, commit.
2. `git tag -a vX.Y.Z -m "noteboard X.Y.Z" && git push origin vX.Y.Z`. The Release
   workflow checks the tag matches `Cargo.toml`, builds macOS and Linux (arm64 and
   x86_64; Linux is static musl) and uploads `.tar.gz` + `.sha256` files.
   To rehearse first without publishing: `gh workflow run release.yml -f tag=vX.Y.Z -f dry_run=true`.
3. `gh workflow run install.yml` to try `scripts/install.sh` against the release
   (a release made by a workflow can't trigger other workflows itself).
4. `just brew-formula X.Y.Z` writes `Formula/noteboard.rb` in `~/Developer/homebrew-tap`.
   Test it from a throwaway tap (`brew tap you/test ~/Developer/homebrew-tap`), then
   commit and push the tap.
