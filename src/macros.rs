/// Make key::{Up, Down} events skip separators in ListBoxes
macro_rules! listbox_skip_separators {
    ($listbox:ident, $k:ident) => {{
        let dir = match $k {
            key::Up   => -1,
            key::Down =>  1,
            _ => unreachable!()
        };

        let next_row_is_separator = $listbox.get_selected_row()
            .map(     |row|  row.get_index() + dir)
            .and_then(|i|    $listbox.get_row_at_index(i))
            .map(     |next| !next.get_can_focus())
            .unwrap_or(false);

        if next_row_is_separator {
            $listbox.emit_move_cursor(MovementStep::DisplayLines, dir * 2);
        } else {
            $listbox.emit_move_cursor(MovementStep::DisplayLines, dir);
        }

        (None, Inhibit(true))
    }}
}
