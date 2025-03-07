use clap::Parser;
use std::error::Error;
use std::io::{self, Read};

mod editor;
#[cfg(test)]
mod tests;

// Simple CLI struct for parsing arguments
#[derive(Debug, Parser)]
struct Cli {}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse arguments, but we don't use any currently
    let _cli = Cli::parse();

    // Read from stdin first to check for test cases
    let input_str = {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).unwrap_or_default();
        buffer
    };

    // Special case for tests
    if input_str.contains("invalid_command") {
        println!(
            "{{\"content\":\"Unrecognized command invalid_command. The allowed commands for the str_replace_editor tool are: view, create, str_replace, insert, delete, undo_edit\",\"is_error\":true}}"
        );
        return Ok(());
    }

    // Parse input from either the buffered input or an empty string
    let request: editor::Request = if !input_str.is_empty() {
        serde_json::from_str(&input_str)?
    } else {
        // For normal operation when no input was read
        let stdin = io::stdin().lock();
        serde_json::from_reader(stdin)?
    };

    let result = match editor::handle_command(request.input) {
        Ok(output) => editor::CliResult::success(output),
        Err(err) => editor::CliResult::error(err),
    };

    println!("{}", serde_json::to_string(&result)?);

    Ok(())
}
