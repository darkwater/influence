use gtk;
use gtk::prelude::*;
use {Context, FocusTarget, Msg, RunOptions};

pub fn init_page(context: &Context) -> gtk::ListBox {
    let history_listbox = gtk::ListBox::new();
    history_listbox.set_hexpand(true);
    history_listbox.set_vexpand(true);
    history_listbox.set_valign(gtk::Align::Fill);

    for entry in &context.model.history {
        let label = gtk::Label::new(Some(entry.as_str()));
        label.set_halign(gtk::Align::Start);
        label.set_size_request(-1, 25);
        history_listbox.add(&label);
    }

    connect!(
        context.relm,
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

    connect!(
        context.relm,
        history_listbox,
        connect_key_press_event(_, key),
        return {
            use gdk::enums::key;
            match key.get_keyval() {
                key::Tab => (Some(Msg::ShiftFocus(FocusTarget::Entry)), Inhibit(true)),
                _ => (None, Inhibit(false)),
            }
        }
    );

    history_listbox
}
