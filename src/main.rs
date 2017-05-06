extern crate gtk;
extern crate gdk;
extern crate gdk_sys;

use gtk::prelude::*;
use std::env;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::ops::Deref;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

macro_rules! clone {
    (@param _ ) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
    ($($n:ident),+ => move |$($p:tt : $z:ty),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p) : $z,)+| $body
        }
    );
}

/// Returns the contents of a file
fn read_file<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut string = String::new();
    let mut file   = File::open(path)?;
    let _          = file.read_to_string(&mut string)?;

    Ok(string)
}

struct CommandListItem {
    widget: Box<gtk::Label>
}

impl CommandListItem {
    fn new(cmd: &str) -> Self {
        let label = gtk::Label::new(Some(cmd));
        label.set_halign(gtk::Align::Start);
        label.set_size_request(-1, 25);

        CommandListItem {
            widget: Box::new(label)
        }
    }
}

impl Deref for CommandListItem {
    type Target = gtk::Label;

    fn deref(&self) -> &Self::Target {
        self.widget.deref()
    }
}

struct CommandList {
    widget: gtk::ScrolledWindow,
    command_list: gtk::ListBox,
    bookmarks: Vec<String>
}

impl CommandList {
    fn new() -> Self {
        let container = gtk::ScrolledWindow::new(None, None);
        container.set_vexpand(true);

        let command_list = gtk::ListBox::new();
        command_list.set_valign(gtk::Align::End);
        command_list.set_selection_mode(gtk::SelectionMode::None);
        container.add(&command_list);

        let bookmarks = {
            let mut bookmark_path = env::home_dir().unwrap();
            bookmark_path.push(".local/share/influence/bookmarks.txt");

            read_file(bookmark_path).unwrap().lines().map(|l| l.to_string()).rev().collect::<Vec<String>>()
        };

        for bookmark in &bookmarks {
            let cmdlist_item = CommandListItem::new(&bookmark);
            command_list.add(&*cmdlist_item);
        }

        CommandList {
            widget: container,
            command_list: command_list,
            bookmarks: bookmarks
        }
    }

    fn filter(&self, substr: String) {
        for (i, bookmark) in self.bookmarks.iter().enumerate() {
            let row = self.command_list.get_row_at_index(i as i32).unwrap();
            match bookmark.contains(&substr) {
                true  => row.show(),
                false => row.hide()
            }
        }
    }
}

impl Deref for CommandList{
    type Target = gtk::ScrolledWindow;

    fn deref(&self) -> &Self::Target {
        &self.widget
    }
}

fn run_command_and_exit(cmd: &str) {
    let _ = Command::new("/bin/bash")
        .arg("-c")
        .arg(cmd)
        .spawn()
        .expect("failed to execute child");

    gtk::main_quit();
}

fn show_window() {
    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_name("influence");
    window.set_type_hint(gdk::WindowTypeHint::Dialog);
    window.set_decorated(false);

    let screen = window.get_screen().unwrap();
    let monitor_id = screen.get_primary_monitor();
    let monitor = screen.get_monitor_geometry(monitor_id);

    let padding = 40;
    let window_width = 600;
    let window_height = 250;
    let window_x = monitor.x + padding;
    let window_y = monitor.y + monitor.height - padding - window_height;

    window.set_app_paintable(true);
    let visual = screen.get_rgba_visual().unwrap();
    window.set_visual(Some(&visual));

    let css_provider = gtk::CssProvider::new();
    let _ = css_provider.load_from_data("
    label {
        font-family: 'Droid Sans Mono';
        font-size: 10pt;
        padding: 5px 10px;
    }
    list > row:focus {
        background: #215d9c;
        outline: none;
    }
    entry {
        font-family: 'Droid Sans Mono';
        padding: 0px 10px;
        font-size: 10pt;
    }
    ");
    gtk::StyleContext::add_provider_for_screen(&screen, &css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.add(&container);

    let cmdlist = Rc::new(CommandList::new());
    container.add(&**cmdlist);

    let input = gtk::Entry::new();
    container.add(&input);

    input.connect_changed(clone!(cmdlist => move |widget| {
        (*cmdlist).filter(widget.get_text().unwrap_or("".to_string()));
    }));

    input.connect_key_press_event(|input, event| {
        use gdk::enums::key;

        match event.get_keyval() {
            key::Return => {
                run_command_and_exit(&input.get_text().unwrap_or("".to_string()));
                Inhibit(true)
            },

            key::Escape => {
                gtk::main_quit();
                Inhibit(true)
            }

            _ => Inhibit(false)
        }
    });

    let bookmarks = cmdlist.bookmarks.clone();
    cmdlist.command_list.connect_key_press_event(clone!(input => move |cmdlist, event| {
        use gdk::enums::key;

        match event.get_keyval() {
            key::Tab => {
                let index = cmdlist.get_children().iter().position(|n| n.has_focus()).unwrap();
                let cmd = &bookmarks[index];
                input.set_text(&cmd);
                input.grab_focus_without_selecting();
                input.set_position(-1);
                Inhibit(true)
            },

            key::Return => {
                let index = cmdlist.get_children().iter().position(|n| n.has_focus()).unwrap();
                let cmd = &bookmarks[index];
                run_command_and_exit(cmd);
                Inhibit(true)
            },

            _ => Inhibit(false)
        }
    }));

    window.show_all();

    input.grab_focus();

    window.move_(window_x, window_y);
    window.set_size_request(window_width, window_height);

    window.get_window().unwrap().set_background_rgba(&gdk::RGBA {
        red:   0x1d as f64 / 255.0,
        green: 0x1f as f64 / 255.0,
        blue:  0x21 as f64 / 255.0,
        alpha: 0xeb as f64 / 255.0
    });

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    gtk::main();
}

fn main() {
    show_window()
}
