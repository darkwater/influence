#![feature(proc_macro)]
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
    Filter(Option<String>),
    MoveBookmarkSelection(i32),
    Quit,
}

struct Win {
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
            Msg::Filter(s)                  => self.filter_bookmarks(s),
            Msg::MoveBookmarkSelection(dir) => self.move_bookmark_selection(dir),
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
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_wmclass("influence", "influence");
        window.set_type_hint(gdk::WindowTypeHint::Dialog);
        window.set_keep_above(true);
        window.set_decorated(false);
        window.stick();

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

        window.set_app_paintable(true);
        let visual = screen.get_rgba_visual().unwrap();
        window.set_visual(Some(&visual));

        let css_provider = gtk::CssProvider::new();
        let _ = css_provider.load_from_data(include_bytes!("main.css"));
        gtk::StyleContext::add_provider_for_screen(&screen, &css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.add(&container);

        // -- Bookmark list --

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

        // -- Command input --

        let input = gtk::Entry::new();
        container.add(&input);

        connect!(
            relm,
            input,
            connect_changed(widget),
            Some(Msg::Filter(widget.get_text()))
        );

        connect!(
            relm,
            input,
            connect_key_press_event(_, key),
            return {
                use gdk::enums::key;
                match key.get_keyval() {
                    key::Up   => (Some(Msg::MoveBookmarkSelection(-1)), Inhibit(true)),
                    key::Down => (Some(Msg::MoveBookmarkSelection( 1)), Inhibit(true)),
                    _         => (None,                                 Inhibit(false)),
                }
            }
        );

        // input.connect_changed(clone!(cmdlist => move |widget| {
        //     (*cmdlist).filter(widget.get_text().unwrap_or("".to_string()));
        // }));

        // input.connect_key_press_event(|input, event| {
        //     use gdk::enums::key;

        //     match event.get_keyval() {
        //         key::Return => {
        //             run_command_and_exit(&input.get_text().unwrap_or("".to_string()));
        //             Inhibit(true)
        //         },

        //         key::Escape => {
        //             gtk::main_quit();
        //             Inhibit(true)
        //         }

        //         _ => Inhibit(false)
        //     }
        // });

        // let bookmarks = cmdlist.bookmarks.clone();
        // cmdlist.command_list.connect_key_press_event(clone!(input => move |cmdlist, event| {
        //     use gdk::enums::key;

        //     match event.get_keyval() {
        //         key::Tab => {
        //             let index = cmdlist.get_children().iter().position(|n| n.has_focus()).unwrap();
        //             let cmd = &bookmarks[index];
        //             input.set_text(&cmd);
        //             input.grab_focus_without_selecting();
        //             input.set_position(-1);
        //             Inhibit(true)
        //         },

        //         key::Return => {
        //             let index = cmdlist.get_children().iter().position(|n| n.has_focus()).unwrap();
        //             let cmd = &bookmarks[index];
        //             run_command_and_exit(cmd);
        //             Inhibit(true)
        //         },

        //         _ => Inhibit(false)
        //     }
        // }));

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

        Win {
            bookmarks_listbox: cmdlist,
            command_entry: input,
            model, window
        }
    }
}

impl Win {
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
            for index in (0..(self.model.bookmarks.len() as i32 - 1)).rev() {
                let row = self.bookmarks_listbox.get_row_at_index(index);
                if let Some(row) = row.and_then(|r| if r.is_visible() { Some(r) } else { None }) {
                    self.bookmarks_listbox.select_row(Some(&row));
                    break;
                }
            }
        }

        // Scroll to the bottom
        if let Some(adj) = self.bookmarks_listbox.get_adjustment() {
            adj.set_value(adj.get_upper());
        }
    }

    fn move_bookmark_selection(&self, dir: i32) {
        self.bookmarks_listbox.emit_move_cursor(gtk::MovementStep::DisplayLines, dir);
        self.command_entry.grab_focus_without_selecting();
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

fn run_command_and_exit(cmd: &str) {
    let _ = Command::new("/bin/bash")
        .arg("-c")
        .arg(cmd)
        .spawn()
        .expect("failed to execute child");

    // TODO: send Quit message
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
