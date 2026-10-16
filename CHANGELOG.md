# Changelog

Notable changes to noteboard. Versions follow [semver](https://semver.org/).

## [Unreleased]

## [0.5.0] - 2026-10-16

First release as a standalone, MIT-licensed project.

### Board
- A folder of Markdown files is the board: each file is a ticket, placed by its
  front matter (`title`, `status`, `priority`, `due`, `tags`, `order`). The folder's
  `README.md` front matter sets the title, columns, done column, sort and archive folder.
- Front matter edits touch only the keys that change; comments, other keys and the
  body are left exactly as written. Files with broken YAML are shown with a warning
  and never rewritten.
- The board reloads by itself when files change on disk.
- Archive moves files to `archive/`; delete moves them to `.trash/`.

### Mouse
- Drag cards between columns, or up and down to put them in your own order
  (saved as `order:`; a "drop here" bar shows where a card will land).
- Double-click a card for the quick-edit form; right-click for its menu.
- Clickable buttons in the top bar, a `[+]` on every column, clickable form fields,
  tag chips, menu items, tabs and dialog buttons.

### Keyboard and settings
- Every shortcut can be rebound and every colour changed in the Options screen (`o`),
  or in `keys.toml` / `theme.toml`. Hints on screen follow your bindings.
- Board settings screen (`b`): rename, add, delete and reorder columns (renames
  carry their tickets along), and set the done column, default sort and archive folder.
- Sorts: due date, priority, title and manual.

### Command line
- Every board operation is a command with `--json` output for scripts and agents:
  `ls`, `show`, `add`, `edit`, `move`, `reorder`, `done`, `archive`, `rm`, `dup`,
  `open`, `path`, `tags`, `columns` (plus `add`/`rename`/`rm`/`move`), `config`, `init`.
- Install with Homebrew (`brew install conorbr/tap/noteboard`) or the install script.
