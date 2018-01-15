extern crate gdk;
extern crate gtk;
#[macro_use]
extern crate relm;
#[macro_use]
extern crate relm_derive;

use gdk::prelude::*;
use gtk::Window;
use gtk::prelude::*;
use relm::{Relm, Update, Widget};
use std::cell::Cell;
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;

const STATE_BOOKMARKS_LABEL: &str = "Bookmarks";
const STATE_MENU: &[(i32, State, &str)] = &[
    (0, State::Bookmarks, STATE_BOOKMARKS_LABEL),
    (1, State::History,   "History"),
];

enum FileStore {
    Bookmarks,
    History,
}

struct Model {
    bookmarks: Vec<String>,
    history: Vec<String>,
    state: Cell<State>,
}

enum State {
    Bookmarks,
    History,
    Searching,
}

#[derive(Msg)]
enum Msg {
    CommandInputChanged(String),
    MoveListSelection(i32),
    RunCommandFromSource(CommandSource, RunOptions),
    RunCommand(String, RunOptions),
    CompleteEntry,
    Quit,
}

enum CommandSource {
    ListSelection(bool), // true to use entry as fallback
    Entry,
}

struct RunOptions {
    /// Whether to quit after running the command
    quit: bool,

    /// Whether to add this command to the history
    record: bool,
}

struct Win {
    relm: Relm<Win>,
    model: Model,
    window: Window,
    bookmarks_listbox: gtk::ListBox,
    command_entry: gtk::Entry,
}

impl Update for Win {
    type Model = Model;
    type ModelParam = ();
    type Msg = Msg;

    fn model(_relm: &Relm<Self>, _param: Self::ModelParam) -> Model {
        let bookmarks = read_file_list(FileStore::Bookmarks).unwrap_or_else(|e| {
            println!("unable to read bookmarks: {}", e);
            Default::default()
        });

        let history = read_file_list(FileStore::History).unwrap_or_else(|e| {
            println!("unable to read history: {}", e);
            Default::default()
        });

        let state = Cell::new(State::Bookmarks);

        Model { bookmarks, history, state }
    }

    fn update(&mut self, event: Self::Msg) {
        match event {
            Msg::CommandInputChanged(s)          => self.command_input_changed(s),
            Msg::MoveListSelection(dir)          => self.move_list_selection(dir),
            Msg::RunCommandFromSource(src, opts) => self.run_command_from_source(src, opts),
            Msg::RunCommand(s, opts)             => self.run_command(s, opts),
            Msg::CompleteEntry                   => self.complete_entry(),
            Msg::Quit                            => gtk::main_quit(),
        }
    }
}

impl Widget for Win {
    type Root = Window;

    fn root(&self) -> Self::Root {
        self.window.clone()
    }

    fn view(relm: &Relm<Self>, model: Self::Model) -> Self {
        // Create window
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_wmclass("influence", "influence");
        window.set_type_hint(gdk::WindowTypeHint::Dialog);
        window.set_keep_above(true);
        window.set_decorated(false);
        window.set_border_width(5);
        window.stick();

        // Position window
        let screen = window.get_screen().unwrap();
        let monitor_id = screen.get_primary_monitor();
        let monitor = screen.get_monitor_geometry(monitor_id);

        let padding = 40;
        let window_width = 600;
        let window_height = 250;
        let window_x = monitor.x + padding;
        let window_y = monitor.y + monitor.height - padding - window_height;
        window.move_(window_x, window_y);
        window.set_default_size(window_width, window_height);

        // // Enable transparency
        // window.set_app_paintable(true);
        // let visual = screen.get_rgba_visual().unwrap();
        // window.set_visual(Some(&visual));

        // Apply custom application CSS
        let css_provider = gtk::CssProvider::new();
        let _ = css_provider.load_from_data(include_bytes!("main.css"));
        gtk::StyleContext::add_provider_for_screen(&screen, &css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

        // Setup UI like this:
        // Window
        //  '- Box #root_container
        //      |- Notebook #notebook
        //      |   '- ScrolledWindow - ListBox #bookmarks_listbox
        //      '- Entry #command_entry
        let root_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root_container.set_spacing(5);
        window.add(&root_container);

        let notebook = gtk::Notebook::new();
        notebook.set_tab_pos(gtk::PositionType::Left);
        root_container.add(&notebook);

        // UI: Bookmark list
        let scroller = gtk::ScrolledWindow::new(None, None);
        let bookmarks_listbox = gtk::ListBox::new();
        bookmarks_listbox.set_hexpand(true);
        bookmarks_listbox.set_vexpand(true);
        bookmarks_listbox.set_valign(gtk::Align::Fill);
        scroller.add(&bookmarks_listbox);
        notebook.add(&scroller);
        notebook.set_tab_label_text(&scroller, STATE_BOOKMARKS_LABEL);

        for bookmark in &model.bookmarks {
            let label = gtk::Label::new(Some(bookmark.as_str()));
            label.set_halign(gtk::Align::Start);
            label.set_size_request(-1, 25);
            bookmarks_listbox.add(&label);
        }

        if model.bookmarks.len() > 0 {
            let last_row = bookmarks_listbox.get_row_at_index(model.bookmarks.len() as i32 - 1);
            bookmarks_listbox.select_row(last_row.as_ref());
        }

        // Scroll to the bottom
        let cmdlist_c = bookmarks_listbox.clone();
        gtk::timeout_add(10, move || {
            if let Some(adj) = cmdlist_c.get_adjustment() {
                adj.set_value(adj.get_upper());
            }
            Continue(false)
        });

        connect!(
            relm,
            bookmarks_listbox,
            connect_row_activated(_, row),
            {
                let label = row.get_child().unwrap().downcast::<gtk::Label>().unwrap();
                let cmd = label.get_text().map(|s| s.to_string()).unwrap_or_default();
                Some(Msg::RunCommand(cmd, RunOptions {
                    quit: true,
                    record: true,
                }))
            }
        );

        // UI: Command input
        let command_entry = gtk::Entry::new();
        root_container.add(&command_entry);

        connect!(
            relm,
            command_entry,
            connect_changed(widget),
            Some(Msg::CommandInputChanged(widget.get_text().unwrap_or_default()))
        );

        connect!(
            relm,
            command_entry,
            connect_key_press_event(_, key),
            return {
                use gdk::enums::key;
                use gdk::ModifierType;
                let state = key.get_state();

                match key.get_keyval() {
                    // Move through list
                    key::Up   => (Some(Msg::MoveListSelection(-1)), Inhibit(true)),
                    key::Down => (Some(Msg::MoveListSelection( 1)), Inhibit(true)),

                    // Run the command
                    key::Return => {
                        // Hold shift to execute command as entered without completing
                        let source = if state.contains(ModifierType::SHIFT_MASK) {
                            CommandSource::Entry
                        } else {
                            CommandSource::ListSelection(true)
                        };

                        let opts = RunOptions {
                            // Hold control to not quit influence afterwards
                            quit: !state.contains(ModifierType::CONTROL_MASK),

                            // Hold alt to not record this command
                            record: !state.contains(ModifierType::MOD1_MASK),
                        };

                        (Some(Msg::RunCommandFromSource(source, opts)), Inhibit(true))
                    },

                    // Fill entry with selected bookmark
                    key::Tab => (Some(Msg::CompleteEntry), Inhibit(true)),

                    _ => (None, Inhibit(false)),
                }
            }
        );

        // Window events
        connect!(
            relm,
            window,
            connect_key_press_event(_, key),
            return {
                use gdk::enums::key;
                match key.get_keyval() {
                    key::Escape => (Some(Msg::Quit), Inhibit(true)),
                    _           => (None,            Inhibit(false)),
                }
            }
        );

        connect!(
            relm,
            window,
            connect_delete_event(_, _),
            return (Some(Msg::Quit), Inhibit(false))
        );

        window.show_all();
        command_entry.grab_focus();

        // window
        //     .get_window()
        //     .unwrap()
        //     .set_background_rgba(&gdk::RGBA {
        //         red: 0x1d as f64 / 255.0,
        //         green: 0x1f as f64 / 255.0,
        //         blue: 0x21 as f64 / 255.0,
        //         alpha: 0xeb as f64 / 255.0,
        //     });

        let relm = relm.clone();

        Win {
            relm, model, window,
            bookmarks_listbox, command_entry,
        }
    }
}

impl Win {
    fn select_bottom_bookmark(&self) {
        for index in (0..(self.model.bookmarks.len() as i32)).rev() {
            let row = self.bookmarks_listbox.get_row_at_index(index);
            if let Some(row) = row.and_then(|r| if r.is_visible() { Some(r) } else { None }) {
                self.bookmarks_listbox.select_row(Some(&row));
                break;
            }
        }
    }

    fn command_input_changed(&self, s: String) {
        let last_state = self.model.state.replace(State::Searching);
        match last_state {
            State::Searching => (),
            _ => {
                
            }
        }
    }

    // FIXME: Doesn't work until the user changes selection manually
    fn move_list_selection(&self, dir: i32) {
        self.bookmarks_listbox.emit_move_cursor(gtk::MovementStep::DisplayLines, dir);
        self.command_entry.grab_focus_without_selecting();
    }

    fn get_selected_bookmark(&self) -> Option<String> {
        self.bookmarks_listbox
            .get_selected_row()
            .and_then(|r| if r.is_visible() { Some(r) } else { None })
            .map(|r| self.model.bookmarks[r.get_index() as usize].clone())
    }

    fn run_command_from_source(&self, source: CommandSource, opts: RunOptions) {
        match source {
            CommandSource::ListSelection(or_entry) => {
                if let Some(bookmark) = self.get_selected_bookmark() {
                    self.run_command(bookmark, opts);
                } else if or_entry {
                    self.run_command_from_source(CommandSource::Entry, opts);
                }
            },
            CommandSource::Entry => {
                if let Some(cmd) = self.command_entry.get_text() {
                    self.run_command(cmd, opts);
                }
            }
        }
    }

    fn run_command(&self, mut cmd: String, opts: RunOptions) {
        cmd.push('&');
        let _ = Command::new("/bin/bash")
            .arg("-c")
            .arg(cmd)
            .spawn()
            .expect("failed to execute child")
            .wait();

        if opts.quit {
            self.relm.stream().emit(Msg::Quit);
        }
    }

    fn complete_entry(&self) {
        if let Some(bookmark) = self.get_selected_bookmark() {
            self.command_entry.set_text(&bookmark);
            self.command_entry.set_position(bookmark.len() as i32);
        }
    }
}

/// Read a list of commands from a file
fn read_file_list(store: FileStore) -> Result<Vec<String>, Box<std::error::Error>> {
    let mut path = PathBuf::from(std::env::var("HOME")?);

    match store {
        FileStore::Bookmarks => path.push(".config/influence/bookmarks.txt"),
        FileStore::History   => path.push(".config/influence/history.txt"),
    }

    let mut string = String::new();
    let mut file = File::open(path)?;
    let _ = file.read_to_string(&mut string)?;
    let bookmarks = string
        .lines()
        .map(|l| l.to_string())
        .collect::<Vec<String>>();

    Ok(bookmarks)
}

fn main() {
    Win::run(()).unwrap();
}
