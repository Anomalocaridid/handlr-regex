use tabled::{
    settings::{themes::Colorization, Alignment, Color, Padding, Style},
    Table, Tabled,
};

/// Render a table from a vector of instances of Tabled structs
pub fn render_table<T: Tabled>(rows: &Vec<T>, terminal_output: bool) -> String {
    let mut table = Table::new(rows);

    if terminal_output {
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
