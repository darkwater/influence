use gtk;
use gtk::prelude::*;
use {CommandSource, Context, Msg, RunOptions};

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

            let state      = ev.get_state();
            let ctrl_held  = state.contains(ModifierType::CONTROL_MASK);
            let shift_held = state.contains(ModifierType::SHIFT_MASK);
            let alt_held   = state.contains(ModifierType::MOD1_MASK);

            match ev.get_keyval() {
                // Move through list
                key::Up   => (Some(Msg::MoveListSelection(-1)), Inhibit(true)),
                key::Down => (Some(Msg::MoveListSelection( 1)), Inhibit(true)),
                // key::Down => (Some(Msg::ShiftFocus(FocusTarget::Notebook)), Inhibit(true)),
                // key::Up   => (Some(Msg::MoveListSelection(ev.clone())), Inhibit(true)),
                // key::Down => (Some(Msg::MoveListSelection(ev.clone())), Inhibit(true)),

                // Run the command
                key::Return => (Some(Msg::RunCommandFromSource(
                            if shift_held {
                                CommandSource::Entry
                            } else {
                                CommandSource::ListSelection(true)
                            },
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
