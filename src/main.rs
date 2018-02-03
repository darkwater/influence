#![feature(nll)]

extern crate itertools;
extern crate gdk;
extern crate gtk;
#[macro_use]
extern crate relm;
#[macro_use]
extern crate relm_derive;

use gdk::prelude::*;
use gtk::Orientation;
use gtk::Window;
use gtk::prelude::*;
use itertools::Itertools;
use relm::{Relm, Update, Widget};
use std::env;
use std::fs::File;
use std::fs;
use std::io::BufWriter;
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

#[macro_use]
mod macros;

mod gui;
mod page;

const BOOKMARKS_LABEL: &str = "Bookmarks";
const HISTORY_LABEL:   &str = "History";
const RESULTS_LABEL:   &str = "Results";
const PROGRAMS_LABEL:  &str = "Programs";

const HISTORY_MAXLEN: usize = 50;

pub enum FileStore {
    Bookmarks,
    History,
}

// Used during gui initialization
pub struct Context<'a> {
    res_scale: &'a Fn(i32) -> i32,
    model:     &'a Model,
    relm:      &'a Relm<Win>,
}

pub struct Model {
    bookmarks:          Vec<String>,
    history:            Vec<String>,
    programs:           Vec<String>,
    focus_results_page: bool,
}

#[derive(Msg)]
pub enum Msg {
    CommandInputChanged(String),
    PageSwitched(gtk::Widget),
    MoveListSelection(i32),
    RemoveHistoryEntry(i32),
    RunCommandFromSource(CommandSource, RunOptions),
    RunCommand(String, RunOptions),
    ShiftFocus(FocusTarget),
    SelectPage(Page),
    CompleteEntry,
    Quit,
}

pub enum Page {
    Abs(i32), // page 1, page 2, ...
    Rel(i32), // next page (1), prev page (-1)
}

pub enum FocusTarget {
    Notebook, // the current tab in the notebook
    Entry,
}

pub enum NotebookTab {
    ListBox(gtk::ListBox),
}

pub enum CommandSource {
    ListSelection(bool), // true to use entry as fallback
    Entry,
}

pub struct RunOptions {
    /// Whether to quit after running the command
    quit: bool,

    /// Whether to add this command to the history
    record: bool,
}

pub struct Win {
    relm:            Relm<Win>,
    model:           Model,
    window:          Window,
    history_listbox: gtk::ListBox,
    results_listbox: gtk::ListBox,
    command_entry:   gtk::Entry,
    notebook:        gtk::Notebook,
    current_tab:     gtk::Widget,
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

        let programs = get_path_commands();

        Model {
            focus_results_page: true,
            bookmarks, history, programs,
        }
    }

    fn update(&mut self, event: Self::Msg) {
        match event {
            Msg::CommandInputChanged(s)          => self.command_input_changed(s),
            Msg::PageSwitched(page)              => self.page_switched(page),
            Msg::MoveListSelection(dir)          => self.move_list_selection(dir),
            Msg::RemoveHistoryEntry(i)           => self.remove_history_entry(i),
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
        let resolution = screen.get_property_resolution() / 96.0;
        let res_scale = |i: i32| ((i as f64) * resolution) as i32;

        let padding = res_scale(40);
        let window_width = res_scale(500);
        let window_height = res_scale(250);
        let window_x = monitor.x + padding;
        let window_y = monitor.y + monitor.height - padding - window_height;
        window.move_(window_x, window_y);
        window.set_default_size(window_width, window_height);
        window.set_border_width(res_scale(5) as u32);

        // Apply custom application CSS
        let css_provider = gtk::CssProvider::new();
        let _ = css_provider.load_from_data(include_bytes!("main.css"));
        gtk::StyleContext::add_provider_for_screen(&screen, &css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

        // Context for initializing the widgets
        let context = Context {
            res_scale: &res_scale,
            model: &model,
            relm
        };

        let root_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root_container.set_spacing(res_scale(5));
        window.add(&root_container);

        let notebook = gtk::Notebook::new();
        notebook.set_tab_pos(gtk::PositionType::Left);
        root_container.add(&notebook);

        connect!(
            relm,
            notebook,
            connect_switch_page(_, widget, _index),
            Some(Msg::PageSwitched(widget.clone()))
        );

        // UI: Bookmarks
        let bookmarks_listbox = page::bookmarks::init_page(&context);
        let scroller = gtk::ScrolledWindow::new(None, None);
        scroller.add(&bookmarks_listbox);
        notebook.add(&scroller);
        notebook.set_tab_label_text(&scroller, BOOKMARKS_LABEL);

        let current_tab = bookmarks_listbox.clone().upcast();

        // UI: History
        let history_listbox = page::history::init_page(&context);
        let scroller = gtk::ScrolledWindow::new(None, None);
        scroller.add(&history_listbox);
        notebook.add(&scroller);
        notebook.set_tab_label_text(&scroller, HISTORY_LABEL);

        // UI: Search results
        let results_listbox = page::results::init_page(&context);
        let scroller = gtk::ScrolledWindow::new(None, None);
        scroller.add(&results_listbox);
        notebook.add(&scroller);
        notebook.set_tab_label_text(&scroller, RESULTS_LABEL);

        // UI: Command input
        let command_entry = gui::init_command_entry(&context);
        root_container.add(&command_entry);

        // Window events
        connect!(
            relm,
            window,
            connect_key_press_event(_, key),
            return {
                use gdk::enums::key;
                use Page::{Abs, Rel};
                let alt_held = key.get_state().contains(gdk::ModifierType::MOD1_MASK);
                match key.get_keyval() {
                    key::Escape           => (Some(Msg::Quit),                Inhibit(true)),
                    key::_1   if alt_held => (Some(Msg::SelectPage(Abs(0))),  Inhibit(true)),
                    key::_2   if alt_held => (Some(Msg::SelectPage(Abs(1))),  Inhibit(true)),
                    key::_3   if alt_held => (Some(Msg::SelectPage(Abs(2))),  Inhibit(true)),
                    key::_4   if alt_held => (Some(Msg::SelectPage(Abs(3))),  Inhibit(true)),
                    key::_5   if alt_held => (Some(Msg::SelectPage(Abs(4))),  Inhibit(true)),
                    key::_6   if alt_held => (Some(Msg::SelectPage(Abs(5))),  Inhibit(true)),
                    key::_7   if alt_held => (Some(Msg::SelectPage(Abs(6))),  Inhibit(true)),
                    key::_8   if alt_held => (Some(Msg::SelectPage(Abs(7))),  Inhibit(true)),
                    key::_9   if alt_held => (Some(Msg::SelectPage(Abs(8))),  Inhibit(true)),
                    key::_0   if alt_held => (Some(Msg::SelectPage(Abs(9))),  Inhibit(true)),
                    key::Up   if alt_held => (Some(Msg::SelectPage(Rel(-1))), Inhibit(true)),
                    key::Down if alt_held => (Some(Msg::SelectPage(Rel( 1))), Inhibit(true)),
                    _                     => (None,                           Inhibit(false)),
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
            label.set_halign(gtk::Align::Start); // Left-align all notebook tab labels
            notebook.set_tab_reorderable(tab, true);
        }

        window.show_all();
        command_entry.grab_focus();

        let relm = relm.clone();

        Win {
            relm, model, window,
            history_listbox, results_listbox, command_entry, notebook,
            current_tab
        }
    }
}

impl Win {
    fn get_current_tab(&self) -> NotebookTab {
        let listbox = self.current_tab.clone()
            .downcast::<gtk::ScrolledWindow>().ok()
            .and_then(|s| s.get_child())
            .and_then(|w| w.downcast::<gtk::Viewport>().ok())
            .and_then(|s| s.get_child())
            .and_then(|w| w.downcast::<gtk::ListBox>().ok());

        if let Some(listbox) = listbox {
            return NotebookTab::ListBox(listbox);
        }

        panic!("unexpected tab type");
    }

    fn page_switched(&mut self, page: gtk::Widget) {
        self.current_tab = page;
    }

    fn shift_focus(&self, target: FocusTarget) {
        match target {
            FocusTarget::Notebook => {
                match self.get_current_tab() {
                    NotebookTab::ListBox(listbox) => {
                        listbox.get_focus_child().map(|w| w.grab_focus());
                    },
                }
            },
            FocusTarget::Entry => {
                self.command_entry.grab_focus();
            }
        }
    }

    fn select_page(&self, page: Page) {
        match page {
            Page::Abs(n)  => self.notebook.set_property_page(n),
            Page::Rel(-1) => self.notebook.prev_page(),
            Page::Rel( 1) => self.notebook.next_page(),
            Page::Rel(_)  => unimplemented!()
        }
    }

    fn command_input_changed(&mut self, s: String) {
        if self.model.focus_results_page {
            self.results_listbox.get_parent()
                .and_then(|viewport| viewport.get_parent())
                .and_then(|scroller| self.notebook.page_num(&scroller))
                .map(     |num|      self.notebook.set_property_page(num as i32));

            self.model.focus_results_page = false;
        }

        for row in self.results_listbox.get_children() {
            self.results_listbox.remove(&row);
        }

        if s == "" { return }

        for bookmark in self.model.bookmarks.iter() {
            if bookmark.contains(&s) {
                let label = gtk::Label::new(Some(bookmark.as_str()));
                label.set_halign(gtk::Align::Start);
                self.results_listbox.add(&label);
            }
        }

        let row   = gtk::ListBoxRow::new();
        let box_  = gtk::Box::new(Orientation::Vertical, 0);
        let sep   = gtk::Separator::new(Orientation::Horizontal);
        let label = gtk::Label::new(Some(HISTORY_LABEL));
        label.set_halign(gtk::Align::Start);
        label.set_size_request(-1, 25);
        row.set_sensitive(false);
        row.set_can_focus(false);
        box_.add(&sep);
        box_.add(&label);
        row.add(&box_);
        row.get_style_context().map(|ctx| ctx.add_class("header"));
        self.results_listbox.add(&row);

        let mut limiter = 0;
        let limit = 5;
        for entry in self.model.history.iter() {
            if entry.contains(&s) {
                limiter += 1;
                if limiter > limit { break; }

                let label = gtk::Label::new(Some(entry.as_str()));
                label.set_halign(gtk::Align::Start);
                self.results_listbox.add(&label);
            }
        }

        let row   = gtk::ListBoxRow::new();
        let box_  = gtk::Box::new(Orientation::Vertical, 0);
        let sep   = gtk::Separator::new(Orientation::Horizontal);
        let label = gtk::Label::new(Some(PROGRAMS_LABEL));
        label.set_halign(gtk::Align::Start);
        label.set_size_request(-1, 25);
        row.set_sensitive(false);
        row.set_can_focus(false);
        box_.add(&sep);
        box_.add(&label);
        row.add(&box_);
        row.get_style_context().map(|ctx| ctx.add_class("header"));
        self.results_listbox.add(&row);

        let mut limiter = 0;
        let limit = 10;
        for entry in self.model.programs.iter() {
            if entry.contains(&s) {
                limiter += 1;
                if limiter > limit { break; }

                let label = gtk::Label::new(Some(entry.as_str()));
                label.set_halign(gtk::Align::Start);
                self.results_listbox.add(&label);
            }
        }

        // Select first focussable (non-header) row
        for row in self.results_listbox.get_children() {
            if row.get_can_focus() {
                self.results_listbox.set_focus_child(&row);
                self.results_listbox.select_row(Some(&row.downcast().unwrap()));
                break;
            }
        }

        self.results_listbox.show_all();
    }

    fn move_list_selection(&self, dir: i32) {
        match self.get_current_tab() {
            NotebookTab::ListBox(listbox) => {
                if listbox.get_selected_row().is_some() {
                    listbox.get_focus_child().map(|w| {
                        w.grab_focus();
                    });
                    listbox.emit_move_cursor(gtk::MovementStep::DisplayLines, dir);
                } else {
                    listbox.get_focus_child().map(|w| {
                        w.grab_focus();
                        listbox.select_row(Some(&w.downcast().unwrap()));
                    });
                }
            },
        }
    }

    fn remove_history_entry(&mut self, index: i32) {
        self.model.history.remove(index as usize);

        self.history_listbox.get_row_at_index(index)
            .map(|r| self.history_listbox.remove(&r));

        self.history_listbox.get_row_at_index(index)
            .or_else(|| self.history_listbox.get_row_at_index(index - 1))
            .map(|r| { self.history_listbox.select_row(&r); r.grab_focus(); });

        if let Err(e) = write_file_list(FileStore::History, &self.model.history) {
            println!("unable to write history: {}", e);
        }
    }

    fn get_selected_command(&self) -> Option<String> {
        match self.get_current_tab() {
            NotebookTab::ListBox(listbox) => listbox
                .get_selected_row()
                .and_then(|row|   if row.is_visible() { Some(row) } else { None })
                .and_then(|row|   row.get_child())
                .and_then(|label| label.downcast::<gtk::Label>().ok())
                .and_then(|label| label.get_text()),
        }
    }

    fn run_command_from_source(&mut self, source: CommandSource, opts: RunOptions) {
        match source {
            CommandSource::ListSelection(or_entry) => {
                if let Some(bookmark) = self.get_selected_command() {
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
                println!("unable to write history: {}", e);
            }
        }

        if opts.quit {
            self.relm.stream().emit(Msg::Quit);
        }
    }

    fn complete_entry(&self) {
        if let Some(bookmark) = self.get_selected_command() {
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

/// Get all executables in the user's PATH
fn get_path_commands() -> Vec<String> {
    env::var("PATH")
        .ok().unwrap_or_else(String::new)
        .split(":")
        .map(fs::read_dir)
        .flat_map(Result::ok)
        .flat_map(Itertools::flatten)
        .filter(|file|
            file.metadata().ok()
                .map_or(false, |meta| meta.permissions().mode() & 0o111 > 0))
        .flat_map(|file| file.file_name().into_string())
        .unique()
        .sorted()
}

fn main() {
    Win::run(()).unwrap();
}
