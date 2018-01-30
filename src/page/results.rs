use gtk;
use gtk::{MovementStep, Orientation};
use gtk::prelude::*;
use {Context, FocusTarget, Msg, RunOptions};

pub fn init_page(context: &Context) -> gtk::ListBox {
    let listbox = gtk::ListBox::new();
    listbox.set_hexpand(true);
    listbox.set_vexpand(true);
    listbox.set_valign(gtk::Align::Fill);

    connect!(
        context.relm,
        listbox,
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
        context.relm,
        listbox,
        connect_key_press_event(listbox, key),
        return {
            use gdk::enums::key;
            match key.get_keyval() {
                key::Tab => (Some(Msg::ShiftFocus(FocusTarget::Entry)), Inhibit(true)),

                k @ key::Up |
                k @ key::Down => listbox_skip_separators!(listbox, k),

                _ => (None, Inhibit(false)),
            }
        }
    );

    listbox
}
