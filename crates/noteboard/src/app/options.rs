//! Options screen: rebind keys and change colours. Changes are saved to
//! `keys.toml` / `theme.toml` as soon as they're made.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::*;
use crate::keys::{Chord, ACTIONS};
use crate::theme::{cycle_colour, SLOTS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptionsTab {
    #[default]
    Keys,
    Colours,
}

#[derive(Debug, Clone, Default)]
pub struct OptionsState {
    pub tab: OptionsTab,
    pub row: usize,
    /// Waiting for the key to bind to the selected action.
    pub capturing: bool,
    pub message: Option<String>,
}

impl App {
    pub fn open_options(&mut self) {
        self.options = OptionsState::default();
        self.mode = AppMode::Options;
    }

    pub fn close_options(&mut self) {
        self.options.capturing = false;
        self.mode = AppMode::Board;
    }

    pub fn options_row_count(&self) -> usize {
        match self.options.tab {
            OptionsTab::Keys => ACTIONS.len(),
            OptionsTab::Colours => SLOTS.len(),
        }
    }

    pub fn options_show_tab(&mut self, tab: OptionsTab) {
        if self.options.tab != tab {
            self.options = OptionsState {
                tab,
                ..OptionsState::default()
            };
        }
    }

    pub fn options_toggle_tab(&mut self) {
        let next = match self.options.tab {
            OptionsTab::Keys => OptionsTab::Colours,
            OptionsTab::Colours => OptionsTab::Keys,
        };
        self.options_show_tab(next);
    }

    pub fn options_move(&mut self, delta: i32) {
        let last = self.options_row_count().saturating_sub(1) as i64;
        self.options.row = (self.options.row as i64 + i64::from(delta)).clamp(0, last) as usize;
    }

    /// Enter: start capturing a key, or step the colour forward.
    pub fn options_activate(&mut self) {
        match self.options.tab {
            OptionsTab::Keys => {
                self.options.capturing = true;
                self.options.message = Some("Press the new key (Esc cancels)".into());
            }
            OptionsTab::Colours => self.options_cycle(true),
        }
    }

    /// Clicking a row selects it; clicking the selected row activates it.
    pub fn options_click_row(&mut self, row: usize) {
        if row == self.options.row {
            self.options_activate();
        } else if row < self.options_row_count() {
            self.options.row = row;
            self.options.capturing = false;
        }
    }

    pub fn options_cycle(&mut self, forward: bool) {
        if self.options.tab != OptionsTab::Colours {
            return;
        }
        if let Some(current) = self.theme.get(self.options.row) {
            self.theme
                .set(self.options.row, cycle_colour(current, forward));
            self.save_theme_now();
        }
    }

    /// The next key pressed while capturing becomes the action's only key.
    pub fn options_capture(&mut self, key: KeyEvent) {
        self.options.capturing = false;
        if key.code == KeyCode::Esc {
            self.options.message = None;
            return;
        }
        let Some(info) = ACTIONS.get(self.options.row) else {
            return;
        };
        let chord = Chord::from_event(&key);
        self.keys.bind(info.action, vec![chord]);
        self.options.message = Some(format!("{}: {}", info.label, chord));
        self.save_keys_now();
    }

    /// `r`: put the selected row back to its default.
    pub fn options_reset_row(&mut self) {
        match self.options.tab {
            OptionsTab::Keys => {
                if let Some(info) = ACTIONS.get(self.options.row) {
                    self.keys.reset(info.action);
                    self.save_keys_now();
                }
            }
            OptionsTab::Colours => {
                if let Some(default) = Theme::default().get(self.options.row) {
                    self.theme.set(self.options.row, default);
                    self.save_theme_now();
                }
            }
        }
        self.options.message = Some("Reset to the default".into());
    }

    /// `R`: put the whole tab back to defaults.
    pub fn options_reset_all(&mut self) {
        match self.options.tab {
            OptionsTab::Keys => {
                self.keys = KeyMap::default();
                self.save_keys_now();
            }
            OptionsTab::Colours => {
                self.theme = Theme::default();
                self.save_theme_now();
            }
        }
        self.options.message = Some("Everything on this tab is back to the defaults".into());
    }

    fn save_keys_now(&mut self) {
        if let Err(e) = crate::keys::save_keys(&self.keys) {
            self.options.message = Some(format!("Couldn't save keys.toml: {}", e));
        }
    }

    fn save_theme_now(&mut self) {
        if let Err(e) = crate::theme::save_theme(&self.theme) {
            self.options.message = Some(format!("Couldn't save theme.toml: {}", e));
        }
    }
}
