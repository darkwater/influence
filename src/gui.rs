use gtk;
use gtk::prelude::*;
use {CommandSource, Context, FocusTarget, Msg, RunOptions};

pub fn init_bookmarks_listbox(context: &Context) -> gtk::ListBox {
    let bookmarks_listbox = gtk::ListBox::new();
    bookmarks_listbox.set_hexpand(true);
    bookmarks_listbox.set_vexpand(true);
    bookmarks_listbox.set_valign(gtk::Align::Fill);

    for bookmark in &context.model.bookmarks {
        let label = gtk::Label::new(Some(bookmark.as_str()));
        label.set_halign(gtk::Align::Start);
        label.set_size_request(-1, 25);
        bookmarks_listbox.add(&label);
    }

    if let Some(first_row) = bookmarks_listbox.get_row_at_index(0) {
        bookmarks_listbox.set_focus_child(&first_row);
    }

    connect!(
        context.relm,
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
        context.relm,
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

    bookmarks_listbox
}

pub fn init_history_listbox(context: &Context) -> gtk::ListBox {
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

    history_listbox
}

pub fn init_command_entry(context: &Context) -> gtk::Entry {
    let command_entry = gtk::Entry::new();
    command_entry.set_size_request(-1, (context.res_scale)(30));

    connect!(
        context.relm,
        command_entry,
        connect_changed(widget),
        Some(Msg::CommandInputChanged(widget.get_text().unwrap_or_default()))
    );

    connect!(
        context.relm,
        command_entry,
        connect_key_press_event(_, ev),
        return {
            use gdk::enums::key;
            use gdk::ModifierType;

            let state     = ev.get_state();
            let ctrl_held = state.contains(ModifierType::CONTROL_MASK);
            let alt_held = state.contains(ModifierType::MOD1_MASK);

            match ev.get_keyval() {
                // Move through list
                key::Up   => (Some(Msg::MoveListSelection(-1)), Inhibit(true)),
                key::Down => (Some(Msg::MoveListSelection( 1)), Inhibit(true)),
                // key::Down => (Some(Msg::ShiftFocus(FocusTarget::Notebook)), Inhibit(true)),
                // key::Up   => (Some(Msg::MoveListSelection(ev.clone())), Inhibit(true)),
                // key::Down => (Some(Msg::MoveListSelection(ev.clone())), Inhibit(true)),

                // Run the command
                key::Return => (Some(Msg::RunCommandFromSource(
                            CommandSource::Entry,
                            RunOptions {
                                quit:   !ctrl_held,
                                record: !alt_held,
                            }
                            )), Inhibit(true)),

                // Fill entry with selected bookmark
                key::Tab => (Some(Msg::CompleteEntry), Inhibit(true)),

                _ => (None, Inhibit(false)),
            }
        }
    );

    command_entry
}
