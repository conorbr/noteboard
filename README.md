# noteboard

A TUI board tracker for people who aren't smart enough to just use vim.

It's written in Rust because [DHH told me to](https://www.youtube.com/watch?v=vDjW_dRyKXY).

<img width="2954" height="2058" alt="image" src="https://github.com/user-attachments/assets/ff5b3c68-43f7-49f9-8211-da89ae4f5d5b" />


`noteboard` turns a folder of Markdown files into a mouse-friendly terminal kanban board.
Each file is a ticket; its YAML front matter says which column it's in. There is no
database — the folder *is* the board, so you can edit tickets in any editor, grep them,
and keep them in git, iCloud or Dropbox.

Everything the board can do is also available as a plain CLI command with `--json`
output, so scripts and AI agents can manage tickets too.

## Install

**Homebrew** (macOS and Linux):

```sh
brew install conorbr/tap/noteboard
```

**Install script** (macOS and Linux, no root needed). Downloads the prebuilt binary for
your platform from the latest GitHub Release, verifies its checksum and puts it in
`~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/conorbr/noteboard/main/scripts/install.sh | sh
```

Set `NOTEBOARD_VERSION=0.4.0` to pin a version or `NOTEBOARD_INSTALL_DIR=/usr/local/bin`
to install elsewhere, e.g. `curl -fsSL ... | NOTEBOARD_INSTALL_DIR=~/bin sh`.
To uninstall, delete the `noteboard` file.

**From source** (needs Rust):

```sh
cargo install --git https://github.com/conorbr/noteboard noteboard
# or, from a clone: cargo install --path crates/noteboard
```

Prefer something shorter to type? Add an alias to your shell config, e.g.
`alias nbd=noteboard`.

## Quick start

```sh
mkdir ~/todo && cd ~/todo
noteboard init --title Personal --columns "Backlog,Todo,In Progress,Waiting,Done"
noteboard add "Call the dentist" --due fri --tag health --priority high
noteboard                       # open the board (or: noteboard ~/todo from anywhere)
```

## Ticket files

Every `*.md` file directly inside the folder is a ticket, except `README.md`.
Hidden files and subfolders (including `archive/` and `.trash/`) are ignored.

```markdown
---
title: Call the dentist
status: todo            # which column; matched ignoring case/spaces/dashes
priority: high          # low | medium | high (default medium)
due: 2026-10-20         # YYYY-MM-DD
tags: [health, errands]
created: 2026-10-16
order: 20               # position in Manual sort (set by dragging)
owner: me               # any other keys are yours; noteboard never touches them
---

Free-form notes, checklists, links...
```

Every field is optional:

- **title** — falls back to the first `# Heading`, then the file name.
- **status** — missing puts the ticket in the first column. A status that matches
  no column also lands in the first column, flagged with a warning.
- Files with no front matter at all still show up; front matter is added the first
  time `noteboard` changes them.
- Files with **invalid YAML** are shown with a warning and are never rewritten — fix
  them in your editor (`v` on the card).

When `noteboard` writes a file it edits **only the keys it is changing**. Comments, unknown
keys, key order and the body are preserved exactly.

## Board config (`README.md`)

The folder's `README.md` front matter configures the board. All keys are optional;
without a README you get Todo / In Progress / Done.

```yaml
---
title: Personal                                        # default: folder name
columns: [Backlog, Todo, In Progress, Waiting, Done]
done_column: Done        # column "archive done" clears (default: last column)
sort: due                # due | priority | title | manual (default: due)
archive_dir: archive     # where archived tickets go (default: archive)
---
```

The rest of the README is yours.

### Manual order

Drag a card up or down to put it exactly where you want it. Each card's position is
saved as an `order:` number in its file. Numbers are spaced out (10, 20, 30...), so a
move normally rewrites only the card you moved; the first reorder in a column numbers
all of its cards once. Reordering switches the board to the **Manual** sort and saves
that as the default (`sort: manual`). In Manual sort, cards without an `order` go below
the ordered ones, sorted by due date, and you can also drop a card at a specific
place in another column. In other sorts, dragging to another column only changes the
column.

### Editing settings

- **In the board:** press `b` (or click **Board** in the top bar). Edit the title, add,
  rename, delete and reorder columns (keys or the `[↑] [↓] [✎] [✕]` buttons on each row),
  pick the done column, default sort and archive folder, then `Ctrl+S` / **Save**.
  **Open README in editor** opens the file itself.
- **From the CLI:** `noteboard config` and `noteboard columns ...` (see below).
- **By hand:** edit `README.md`; an open board picks up the change within a second.

Renaming a column (in the board or with `noteboard columns rename`) also rewrites the
`status` of its tickets, so nothing is orphaned. A column can only be deleted once it
is empty. Only the settings you change are written; the rest of the README is left as is.
Tickets without a `status` always appear in whichever column is first.

## The board (TUI)

```
noteboard            # board for the current folder
noteboard ~/todo     # board for another folder
```

The board reloads automatically when files change on disk, so you can keep it open
while editing tickets elsewhere (or while an agent works through the CLI).

### Mouse

| Action | Mouse |
|--------|-------|
| Select a ticket | Click |
| Quick edit (title, notes, status, priority, due, tags) | Double-click |
| Move to another column | Drag and drop |
| Reorder within a column | Drag up or down; a **drop here** bar shows where it lands |
| Ticket menu (edit, open in editor, move to..., priority, duplicate, archive, delete) | Right-click |
| New ticket in a column | Click `[+]` in the column header |
| Scroll a column | Scroll wheel |
| New / Search / Sort / Options / Quit | Buttons in the top bar |
| Edit form | Click a field to focus it; click Status/Priority again to cycle; click tag chips to toggle |
| Dialogs | Click the buttons; click outside to dismiss menus |

### Keyboard

| Key | Action |
|-----|--------|
| `j` / `l` / `←` / `→` | Focus column left / right |
| `↑` / `↓` / `Tab` / `Shift+Tab` | Move cursor |
| `Enter` / `e` | Quick edit: title, notes, status, priority, due, tags |
| `v` | Open the file in `$VISUAL` / `$EDITOR` (default: `nano`) |
| `Space` | New ticket in the focused column |
| `i` | Ticket details |
| `m` | Ticket menu |
| `k` | Pick up ticket, then `j` / `l` to move it between columns, `↑` / `↓` to reorder, `k` / `Esc` to drop |
| `p` | Cycle priority |
| `c` | Duplicate |
| `a` | Archive (moves the file to `archive/`) |
| `d` | Delete (moves the file to `.trash/`) |
| `Ctrl+D` | Archive everything in the done column |
| `/` | Search (title, notes and id) |
| `s` | Sort / filter by tag |
| `b` | Board settings (title, columns, done column, sort, archive folder) |
| `Ctrl+R` | Reload from disk |
| `o` | Options (keybindings and theme) |
| `q` / `Esc` | Quit |

In the edit form: `Tab` / `Shift+Tab` (or `↑` / `↓`) move between fields, `Enter`
adds a newline in notes, `←` / `→` / `Space` cycle Status and Priority, `Ctrl+S` saves.
The due field takes `2026-10-20`, `today`, `tomorrow`, `fri`, `+3d`, `+2w`, `+1m`.

### Keys and colours

Press `o` (or click **Options**) to rebind any shortcut or change any colour; changes
are saved straight away. They live in your config directory
(`~/Library/Application Support/noteboard/` on macOS, `~/.config/noteboard/` on Linux):

```toml
# keys.toml: only the actions you change; each takes one key or a list
[board]
new = "n"
left = ["h", "left"]
[form]
save = "ctrl+s"
```

```toml
# theme.toml: a colour name, a 256-colour number or "#rrggbb"
[board]
card = "darkgray"
cursor = "lightcyan"
[priority]
high = "#ff5f5f"
```

`noteboard keys` and `noteboard theme` print every setting with its default;
add `--init` to write that template to the config directory.

## CLI

Ticket IDs are file names without `.md` (`call-the-dentist`); any unique prefix works
(`noteboard show call-the`). Subcommands use the current folder, or `-C <dir>` / `$NOTEBOARD_DIR`.

```
noteboard ls [-s STATUS]... [-t TAG]... [-q TEXT] [--sort due|priority|title] [--json [--body]]
noteboard show ID [--json]
noteboard add TITLE... [-s STATUS] [-p PRIORITY] [-d DUE] [-t TAG]... [-b BODY | --body-file FILE|-] [--json]
noteboard edit ID [--title T] [-s STATUS] [-p PRIORITY] [-d DUE | --no-due]
           [-t TAG]... [--untag TAG]... [--tags a,b] [-b BODY | --body-file FILE|- | --append TEXT] [--json]
noteboard move ID STATUS [--json]
noteboard reorder ID POSITION [--json] # 1 = top of its column; switches to Manual sort
noteboard done ID... [--json]                 # move to the done column
noteboard archive ID... | --done [--json]     # move files to archive/
noteboard rm ID... [--json]                   # move files to .trash/
noteboard dup ID [--json]
noteboard open ID                             # $VISUAL / $EDITOR
noteboard path [ID]                           # file path (or the board folder)
noteboard tags [--json]
noteboard columns [--json]
noteboard columns add NAME [--at POS]  # POS 1 = leftmost; default: before the done column
noteboard columns rename COLUMN NEW    # also updates the status of its tickets
noteboard columns rm COLUMN            # column must be empty
noteboard columns move COLUMN POS
noteboard config [--json]              # show board settings
noteboard config [--title T | --no-title] [--done-column C] [--sort S | --no-sort] [--archive-dir D]
noteboard init [--title T] [--columns "A,B,C"]
noteboard completions SHELL | noteboard manpage | noteboard theme [--init] | noteboard keys [--init]
```

### For scripts and agents

- `--json` on any read or write command prints the affected ticket(s) as JSON:
  `id`, `title`, `status` (the value written to front matter), `column`,
  `column_index`, `priority`, `due`, `tags`, `path`, `created`, `modified`,
  `warning`, the full `front_matter`, and `body` (`show`/`add`/`edit`, or `ls --body`).
- `noteboard config --json` shows the board settings; `noteboard columns --json` describes the board: title, folder and each column's name,
  `status` value, done flag and ticket count.
- Errors go to stderr as `noteboard: error: ...` with exit code 1 (2 for usage errors).
  Unknown statuses list the valid ones; ambiguous IDs list the candidates.
- Writes are atomic (temp file + rename) and always re-read the file first, so it is
  safe to run commands while the board is open.

```sh
noteboard ls --json | jq -r '.[] | select(.due != null and .status != "done") | .id'
noteboard add "Investigate flaky test" -s todo -t work --body-file notes.md --json
noteboard edit investigate --append "Root cause: race in setup()" && noteboard done investigate
```

## Development

See [CLAUDE.md](CLAUDE.md). Short version: `just check` runs fmt, clippy and tests.

## License

[MIT](LICENSE)
