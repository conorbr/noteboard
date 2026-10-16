mod app;
mod cli;
mod dates;
mod editor;
mod frontmatter;
mod keys;
mod model;
mod ordering;
mod settings;
mod store;
mod theme;
mod ui;

use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::DefaultTerminal;

use app::App;
use cli::{Cli, Commands};

fn main() {
    if let Err(e) = run() {
        eprintln!("noteboard: error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> cli::CliResult {
    let args = Cli::parse();
    match args.command {
        Some(Commands::Completions { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "noteboard", &mut io::stdout());
        }
        Some(Commands::Manpage) => {
            clap_mangen::Man::new(Cli::command()).render(&mut io::stdout())?
        }
        Some(Commands::Theme { init }) => {
            let text = theme::Theme::default().to_toml();
            write_or_print(&theme::theme_path(), &text, init)?;
        }
        Some(Commands::Keys { init }) => {
            let text = keys::KeyMap::default().to_toml(true);
            write_or_print(&keys::keys_path(), &text, init)?;
        }
        Some(command) => cli::run(cli::board_dir(args.dir)?, command)?,
        None => board(&cli::board_dir(args.path.or(args.dir))?)?,
    }
    Ok(())
}

/// Print a config template, or with `--init` write it to its usual place.
fn write_or_print(path: &Path, text: &str, write: bool) -> io::Result<()> {
    if !write {
        print!("{}", text);
        return Ok(());
    }
    if path.exists() {
        return Err(io::Error::other(format!(
            "{} already exists; delete it first to start again",
            path.display()
        )));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)?;
    println!("Wrote {}", path.display());
    Ok(())
}

/// Open the board in the terminal until the user quits.
fn board(dir: &Path) -> cli::CliResult {
    let store = store::Store::open(dir)?;
    let (key_map, key_problems) = keys::load_keys();
    let mut app = App::new(store, theme::load_theme(), key_map);
    if let Some(problem) = key_problems.first() {
        app.set_error(problem.clone());
    } else if app.tasks.is_empty() && !app.store.readme_path().exists() {
        app.set_flash("Nothing here yet: click [+] or press Space to add a ticket".into());
    }

    // Put the terminal back however we leave, panics included.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave_tui();
        previous(info);
    }));

    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = event_loop(&mut terminal, &mut app);
    leave_tui();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> cli::CliResult {
    while app.running {
        let size = terminal.size()?;
        app.terminal_width = size.width;
        app.terminal_height = size.height;
        // Rows inside a column: everything but the bars and the column's own border.
        let rows = size.height.saturating_sub(ui::chrome_height(app) + 2);
        app.update_scroll(rows as usize);

        terminal.draw(|frame| ui::render(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                _ => {}
            }
        }

        if let Some(path) = app.pending_editor.take() {
            leave_tui();
            let outcome = editor::open(&path);
            enter_tui(terminal)?;
            app.editor_finished(outcome);
        }

        app.tick();
    }
    Ok(())
}

fn leave_tui() {
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    let _ = io::stdout().flush();
}

fn enter_tui(terminal: &mut DefaultTerminal) -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.clear()
}
