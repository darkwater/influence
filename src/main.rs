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
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;

pub struct Model {
    bookmarks: Vec<String>,
}

#[derive(Msg)]
pub enum Msg {
    FilterBookmarks(Option<String>),
    MoveBookmarkSelection(i32),
    RunCommand(bool, CommandSource),
    Quit,
}

pub enum CommandSource {
    BookmarkSelection(bool), // true to use entry as fallback
    Entry,
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
        let bookmarks = read_bookmarks().unwrap_or_else(|e| {
            println!("unable to read bookmarks: {}", e);
            Default::default()
        });

        Model { bookmarks }
    }

    fn update(&mut self, event: Self::Msg) {
        match event {
            Msg::FilterBookmarks(substr)    => self.filter_bookmarks(substr),
            Msg::MoveBookmarkSelection(dir) => self.move_bookmark_selection(dir),
            Msg::RunCommand(exit, source)   => self.run_command_from_source(source, exit),
            Msg::Quit                       => gtk::main_quit(),
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

        let padding = 40;
        let window_width = 600;
        let window_height = 250;
        let window_x = monitor.x + padding;
        let window_y = monitor.y + monitor.height - padding - window_height;
        window.move_(window_x, window_y);
        window.set_size_request(window_width, window_height);

        // Enable transparency
        window.set_app_paintable(true);
        let visual = screen.get_rgba_visual().unwrap();
        window.set_visual(Some(&visual));

        // Apply custom application CSS
        let css_provider = gtk::CssProvider::new();
        let _ = css_provider.load_from_data(include_bytes!("main.css"));
        gtk::StyleContext::add_provider_for_screen(&screen, &css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

        // Setup UI
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.add(&container);

        // UI: Bookmark list
        let scroller = gtk::ScrolledWindow::new(None, None);
        let cmdlist = gtk::ListBox::new();
        cmdlist.set_vexpand(true);
        cmdlist.set_valign(gtk::Align::End);
        scroller.add(&cmdlist);
        container.add(&scroller);

        for bookmark in &model.bookmarks {
            let label = gtk::Label::new(Some(bookmark.as_str()));
            label.set_halign(gtk::Align::Start);
            label.set_size_request(-1, 25);
            cmdlist.add(&label);
        }

        if model.bookmarks.len() > 0 {
            let last_row = cmdlist.get_row_at_index(model.bookmarks.len() as i32 - 1);
            cmdlist.select_row(last_row.as_ref());
        }

        // UI: Command input
        let input = gtk::Entry::new();
        container.add(&input);

        connect!(
            relm,
            input,
            connect_changed(widget),
            Some(Msg::FilterBookmarks(widget.get_text()))
        );

        connect!(
            relm,
            input,
            connect_key_press_event(_, key),
            return {
                use gdk::enums::key;
                use gdk::ModifierType;
                let state = key.get_state();

                match key.get_keyval() {
                    key::Up     => (Some(Msg::MoveBookmarkSelection(-1)), Inhibit(true)),
                    key::Down   => (Some(Msg::MoveBookmarkSelection( 1)), Inhibit(true)),
                    key::Return => if state.is_empty() {
                        (Some(Msg::RunCommand(true, CommandSource::BookmarkSelection(true))), Inhibit(true))
                    } else if state == ModifierType::SHIFT_MASK {
                        (Some(Msg::RunCommand(true, CommandSource::Entry)), Inhibit(true))
                    } else {
                        (None, Inhibit(false))
                    },
                    _           => (None, Inhibit(false)),
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
        input.grab_focus();

        window
            .get_window()
            .unwrap()
            .set_background_rgba(&gdk::RGBA {
                red: 0x1d as f64 / 255.0,
                green: 0x1f as f64 / 255.0,
                blue: 0x21 as f64 / 255.0,
                alpha: 0xeb as f64 / 255.0,
            });

        let relm = relm.clone();

        Win {
            bookmarks_listbox: cmdlist,
            command_entry: input,
            relm, model, window
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

    fn filter_bookmarks(&self, substr: Option<String>) {
        let substr = substr.unwrap_or_default();
        for (i, bookmark) in self.model.bookmarks.iter().enumerate() {
            let row = self.bookmarks_listbox.get_row_at_index(i as i32).unwrap();
            match bookmark.contains(&substr) {
                true  => row.show(),
                false => row.hide(),
            }
        }

        // If the current selection went invisible, select the new bottom-most entry
        let selected = self.bookmarks_listbox.get_selected_row();
        if selected.is_none() || !selected.unwrap().is_visible() {
            self.select_bottom_bookmark();
        }

        // Scroll to the bottom
        let listbox = self.bookmarks_listbox.clone();
        gtk::timeout_add(10, move || {
            if let Some(adj) = listbox.get_adjustment() {
                adj.set_value(adj.get_upper());
            }
            Continue(false)
        });
    }

    fn move_bookmark_selection(&self, dir: i32) {
        self.bookmarks_listbox.emit_move_cursor(gtk::MovementStep::DisplayLines, dir);
        self.command_entry.grab_focus_without_selecting();
    }

    fn run_command_from_source(&self, source: CommandSource, exit: bool) {
        match source {
            CommandSource::BookmarkSelection(or_entry) => {
                let selected_row = self.bookmarks_listbox
                    .get_selected_row()
                    .and_then(|r| if r.is_visible() { Some(r) } else { None })
                    .map(|r| r.get_index());

                if let Some(index) = selected_row {
                    let bookmark = self.model.bookmarks[index as usize].clone();
                    self.run_command(bookmark, exit);
                } else if or_entry {
                    self.run_command_from_source(CommandSource::Entry, exit);
                }
            },
            CommandSource::Entry => {
                if let Some(cmd) = self.command_entry.get_text() {
                    self.run_command(cmd, exit);
                }
            }
        }
    }

    fn run_command(&self, cmd: String, exit: bool) {
        let _ = Command::new("/bin/bash")
            .arg("-c")
            .arg(cmd)
            .spawn()
            .expect("failed to execute child");

        if exit {
            self.relm.stream().emit(Msg::Quit);
        }
    }
}

/// Read bookmarks from configuration file
fn read_bookmarks() -> Result<Vec<String>, Box<std::error::Error>> {
    let mut path = PathBuf::from(std::env::var("HOME")?);
    path.push(".config/influence/bookmarks.txt");

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

// macro_rules! clone {
//     (@param _ ) => ( _ );
//     (@param $x:ident) => ( $x );
//     ($($n:ident),+ => move || $body:expr) => (
//         {
//             $( let $n = $n.clone(); )+
//             move || $body
//         }
//     );
//     ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
//         {
//             $( let $n = $n.clone(); )+
//             move |$(clone!(@param $p),)+| $body
//         }
//     );
//     ($($n:ident),+ => move |$($p:tt : $z:ty),+| $body:expr) => (
//         {
//             $( let $n = $n.clone(); )+
//             move |$(clone!(@param $p) : $z,)+| $body
//         }
//     );
// }
