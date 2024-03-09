use std::io::IsTerminal;

use tabled::{
    settings::{themes::Colorization, Alignment, Color, Padding, Style},
    Table, Tabled,
};

pub fn render_table<T: Tabled>(rows: &Vec<T>) -> String {
    let mut table = Table::new(rows);

    if std::io::stdout().is_terminal() {
        // If output is going to a terminal, print as a table
        table
            .with(Style::sharp())
            .with(Colorization::rows([Color::FG_WHITE, Color::BG_BLACK]))
    } else {
        // If output is being piped, print as tab-delimited text
        table
            .with(Style::empty().vertical('\t'))
            .with(Alignment::left())
            .with(Padding::zero())
    }
    .to_string()
}
