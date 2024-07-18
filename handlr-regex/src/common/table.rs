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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use itertools::Itertools;

    #[derive(Tabled)]
    struct TestRow<'a> {
        col1: &'a str,
        col2: &'a str,
    }

    // Arbitrary sample text
    const LOREM_IPSUM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

    // Helper function to create test data
    fn rows(test_text: &str) -> Vec<TestRow<'_>> {
        test_text
            .split(' ')
            .collect_vec()
            .chunks_exact(2)
            .map(|chunk| TestRow {
                col1: chunk[0],
                col2: chunk[1],
            })
            .collect_vec()
    }

    #[test]
    fn terminal_output() -> Result<()> {
        goldie::assert!(render_table(&rows(LOREM_IPSUM), true));
        Ok(())
    }

    #[test]
    fn piped_output() -> Result<()> {
        goldie::assert!(render_table(&rows(LOREM_IPSUM), false));
        Ok(())
    }
}
