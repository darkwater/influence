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
use std::fs::File;
use std::io::BufWriter;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;

const MENU_BOOKMARKS_LABEL: &str = "Bookmarks";
const MENU_HISTORY_LABEL:   &str = "History";

const HISTORY_MAXLEN: usize = 5;

enum FileStore {
    Bookmarks,
    History,
}

struct Model {
    bookmarks: Vec<String>,
    history: Vec<String>,
}

#[derive(Msg)]
enum Msg {
    CommandInputChanged(String),
    MoveListSelection(i32),
    RunCommandFromSource(CommandSource, RunOptions),
    RunCommand(String, RunOptions),
    ShiftFocus(FocusTarget),
    SelectPage(i32),
    CompleteEntry,
    Quit,
}

enum FocusTarget {
    Notebook, // the current tab in the notebook
    Entry,
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
    notebook: gtk::Notebook,
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

        Model { bookmarks, history }
    }

    fn update(&mut self, event: Self::Msg) {
        match event {
            Msg::CommandInputChanged(s)          => self.command_input_changed(s),
            Msg::MoveListSelection(dir)          => self.move_list_selection(dir),
            Msg::RunCommandFromSource(src, opts) => self.run_command_from_source(src, opts),
            Msg::RunCommand(s, opts)             => self.run_command(s, opts),
            Msg::ShiftFocus(target)              => self.shift_focus(target),
            Msg::SelectPage(page)                => self.select_page(page),
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
        window.stick();

        // Position window
        let screen = window.get_screen().unwrap();
        let monitor_id = screen.get_primary_monitor();
        let monitor = screen.get_monitor_geometry(monitor_id);
        let resolution = (screen.get_property_resolution() / 96.0)
            / (screen.get_monitor_scale_factor(monitor_id) as f64);
        let res_scale = |i: i32| ((i as f64) * resolution) as i32;

        let padding = res_scale(40);
        let window_width = res_scale(500);
        let window_height = res_scale(250);
        let window_x = monitor.x + padding;
        let window_y = monitor.y + monitor.height - padding - window_height;
        window.move_(window_x, window_y);
        window.set_default_size(window_width, window_height);
        window.set_border_width(res_scale(5) as u32);

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
        root_container.set_spacing(res_scale(5));
        window.add(&root_container);

        let notebook = gtk::Notebook::new();
        notebook.set_tab_pos(gtk::PositionType::Left);
        root_container.add(&notebook);

        // UI: Bookmarks
        let scroller = gtk::ScrolledWindow::new(None, None);
        let bookmarks_listbox = gtk::ListBox::new();
        bookmarks_listbox.set_hexpand(true);
        bookmarks_listbox.set_vexpand(true);
        bookmarks_listbox.set_valign(gtk::Align::Fill);
        scroller.add(&bookmarks_listbox);
        notebook.add(&scroller);
        notebook.set_tab_label_text(&scroller, MENU_BOOKMARKS_LABEL);

        for bookmark in &model.bookmarks {
            let label = gtk::Label::new(Some(bookmark.as_str()));
            label.set_halign(gtk::Align::Start);
            label.set_size_request(-1, 25);
            bookmarks_listbox.add(&label);
        }

        if let Some(first_row) = bookmarks_listbox.get_row_at_index(0) {
            bookmarks_listbox.set_focus_child(&first_row);
        }

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

        connect!(
            relm,
            bookmarks_listbox,
            connect_key_press_event(_, key),
            return {
                use gdk::enums::key;
                match key.get_keyval() {
                    key::Tab => (Some(Msg::ShiftFocus(FocusTarget::Entry)), Inhibit(true)),
                    _ => (None, Inhibit(false)),
                }
            }
        );

        // UI: History
        let scroller = gtk::ScrolledWindow::new(None, None);
        let history_listbox = gtk::ListBox::new();
        history_listbox.set_hexpand(true);
        history_listbox.set_vexpand(true);
        history_listbox.set_valign(gtk::Align::Fill);
        scroller.add(&history_listbox);
        notebook.add(&scroller);
        notebook.set_tab_label_text(&scroller, MENU_HISTORY_LABEL);

        for entry in &model.history {
            let label = gtk::Label::new(Some(entry.as_str()));
            label.set_halign(gtk::Align::Start);
            label.set_size_request(-1, 25);
            history_listbox.add(&label);
        }

        // if model.bookmarks.len() > 0 {
        //     let last_row = history_listbox.get_row_at_index(model.bookmarks.len() as i32 - 1);
        //     history_listbox.select_row(last_row.as_ref());
        // }

        // // Scroll to the bottom
        // let cmdlist_c = history_listbox.clone();
        // gtk::timeout_add(10, move || {
        //     if let Some(adj) = cmdlist_c.get_adjustment() {
        //         adj.set_value(adj.get_upper());
        //     }
        //     Continue(false)
        // });

        connect!(
            relm,
            history_listbox,
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
        command_entry.set_size_request(-1, res_scale(30));
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
                    // key::Up   => (Some(Msg::MoveListSelection(-1)), Inhibit(true)),
                    // key::Down => (Some(Msg::MoveListSelection( 1)), Inhibit(true)),
                    key::Down => (Some(Msg::ShiftFocus(FocusTarget::Notebook)), Inhibit(true)),

                    // Run the command
                    key::Return => {
                        let source = CommandSource::Entry;

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
                let alt_held = key.get_state().contains(gdk::ModifierType::MOD1_MASK);
                match key.get_keyval() {
                    key::Escape         => (Some(Msg::Quit),          Inhibit(true)),
                    key::_1 if alt_held => (Some(Msg::SelectPage(0)), Inhibit(true)),
                    key::_2 if alt_held => (Some(Msg::SelectPage(1)), Inhibit(true)),
                    _                   => (None,                     Inhibit(false)),
                }
            }
        );

        connect!(
            relm,
            window,
            connect_delete_event(_, _),
            return (Some(Msg::Quit), Inhibit(false))
        );

        for tab in notebook.get_children().iter() {
            let label = notebook.get_tab_label(tab).unwrap();
            label.set_halign(gtk::Align::Start);
        }

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
            bookmarks_listbox, command_entry, notebook,
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

    fn shift_focus(&self, target: FocusTarget) {
        match target {
            FocusTarget::Notebook => {
                if let Some(row) = self.bookmarks_listbox.get_focus_child() {
                    row.grab_focus();
                } else if let Some(row) = self.bookmarks_listbox.get_selected_row() {
                    row.grab_focus();
                } else {
                    self.bookmarks_listbox.emit_move_cursor(gtk::MovementStep::DisplayLines, 0);
                }
            },
            FocusTarget::Entry => {
                self.command_entry.grab_focus();
            }
        }
    }

    fn select_page(&self, page: i32) {
        self.notebook.set_property_page(page);
    }

    fn command_input_changed(&mut self, s: String) {
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

    fn run_command_from_source(&mut self, source: CommandSource, opts: RunOptions) {
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

    fn run_command(&mut self, cmd: String, opts: RunOptions) {
        let _ = Command::new("/bin/bash")
            .arg("-c")
            .arg(format!("{} &", cmd))
            .spawn()
            .expect("failed to execute child")
            .wait();

        if opts.record {
            self.model.history.retain(|c| c != &cmd);
            self.model.history.insert(0, cmd);
            self.model.history.truncate(HISTORY_MAXLEN);
            if let Err(e) = write_file_list(FileStore::History, &self.model.history) {
                println!("unable to read bookmarks: {}", e);
            }
        }

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

fn write_file_list(store: FileStore, list: &Vec<String>) -> Result<(), Box<std::error::Error>> {
    let mut path = PathBuf::from(std::env::var("HOME")?);

    match store {
        FileStore::Bookmarks => path.push(".config/influence/bookmarks.txt"),
        FileStore::History   => path.push(".config/influence/history.txt"),
    }

    let file       = File::create(path)?;
    let mut writer = BufWriter::new(file);

    for line in list.iter() {
        writer.write(line.as_bytes())?;
        writer.write("\n".as_bytes())?;
    }

    Ok(())
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
