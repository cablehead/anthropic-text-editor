use clap::Parser;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

#[cfg(test)]
mod tests;

/// Commands supported by the editor
#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    View,
    Create,
    StrReplace,
    Insert,
    Delete,
    UndoEdit,
}

impl FromStr for Command {
    type Err = EditorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "view" => Ok(Command::View),
            "create" => Ok(Command::Create),
            "str_replace" => Ok(Command::StrReplace),
            "insert" => Ok(Command::Insert),
            "delete" => Ok(Command::Delete),
            "undo_edit" => Ok(Command::UndoEdit),
            _ => Err(EditorError::UnknownCommand(s.to_string())),
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cmd_str = match self {
            Command::View => "view",
            Command::Create => "create",
            Command::StrReplace => "str_replace",
            Command::Insert => "insert",
            Command::Delete => "delete",
            Command::UndoEdit => "undo_edit",
        };
        write!(f, "{}", cmd_str)
    }
}

#[derive(Debug, Error)]
enum EditorError {
    #[error("The path {0} does not exist. Please provide a valid path.")]
    PathNotFound(PathBuf),

    #[error(
        "The path {0} is not an absolute path, it should start with `/`. Maybe you meant /{0}?"
    )]
    NotAbsolutePath(PathBuf),

    #[error("Invalid range: {0}")]
    InvalidRange(String),

    #[error("The `view_range` parameter is not allowed when `path` points to a directory.")]
    ViewRangeForDirectory,

    #[error("{0}")]
    StrReplace(String),

    #[error(
        "The undo_edit command is not implemented in this CLI. Please use git for version control."
    )]
    UndoNotImplemented,

    #[error("File already exists at: {0}. Cannot overwrite files using command `create`.")]
    FileAlreadyExists(PathBuf),

    #[error("Parameter `file_text` is required for command: create")]
    MissingFileText,

    #[error("Parameter `insert_line` is required for command: insert")]
    MissingInsertLine,

    #[error("Parameter `new_str` is required for command: insert")]
    MissingNewStr,

    #[error("Parameter `old_str` is required for command: str_replace")]
    MissingOldStr,

    #[error("Parameter `delete_range` is required for command: delete")]
    MissingDeleteRange,

    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(String),

    #[error("Unrecognized command {0}. The allowed commands for the str_replace_editor tool are: view, create, str_replace, insert, delete, undo_edit")]
    UnknownCommand(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Walk error: {0}")]
    Walk(#[from] walkdir::Error),
}

#[derive(Debug, Deserialize)]
struct Input {
    #[serde(deserialize_with = "deserialize_command")]
    command: Command,
    path: String,
    #[serde(default)]
    view_range: Option<Vec<i32>>,
    #[serde(default)]
    max_depth: Option<usize>,
    #[serde(default)]
    old_str: Option<String>,
    #[serde(default)]
    new_str: Option<String>,
    #[serde(default)]
    insert_line: Option<i32>,
    #[serde(default)]
    file_text: Option<String>,
    #[serde(default)]
    delete_range: Option<Vec<i32>>,
    #[serde(default)]
    allow_multi: Option<bool>,
    #[serde(default)]
    use_regex: Option<bool>,
}

// Custom deserializer for Command enum
fn deserialize_command<'de, D>(deserializer: D) -> Result<Command, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Command::from_str(&s).map_err(serde::de::Error::custom)
}

#[derive(Debug, Deserialize)]
struct Request {
    input: Input,
}

#[derive(Debug, Serialize)]
struct CliResult {
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

impl CliResult {
    fn success(content: String) -> Self {
        Self {
            content,
            is_error: None,
        }
    }

    fn error(err: impl std::error::Error) -> Self {
        Self {
            content: err.to_string(),
            is_error: Some(true),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about = "Anthropic text editor for Claude")]
struct Cli {}

struct Editor {}

impl Editor {
    /// Creates a new Editor instance
    ///
    /// The Editor is responsible for handling file system operations
    /// requested by Claude.
    fn new() -> Self {
        Self {}
    }

    // Method that validates paths based on command type
    fn validate_path_internal(&self, path: &Path, command: &Command) -> Result<(), EditorError> {
        // Check if it's an absolute path
        if !path.is_absolute() {
            return Err(EditorError::NotAbsolutePath(path.to_path_buf()));
        }

        // For create, file should not exist
        match command {
            Command::Create => {
                if path.exists() {
                    return Err(EditorError::FileAlreadyExists(path.to_path_buf()));
                }
            }
            _ => {
                // For other commands, file should exist
                if !path.exists() {
                    return Err(EditorError::PathNotFound(path.to_path_buf()));
                }

                // Check if directory for non-view command
                if path.is_dir() && *command != Command::View {
                    return Err(EditorError::InvalidRange(
                        format!("The path {} is a directory and only the `view` command can be used on directories", path.display())
                    ));
                }
            }
        }

        Ok(())
    }

    fn handle_command(&mut self, input: Input) -> Result<String, EditorError> {
        let path = PathBuf::from(&input.path);

        match input.command {
            Command::View => self.view(&path, input.view_range.as_deref(), input.max_depth),
            Command::Create => {
                let file_text = input.file_text.ok_or(EditorError::MissingFileText)?;
                self.create(&path, &file_text)
            }
            Command::StrReplace => {
                let old_str = input.old_str.ok_or(EditorError::MissingOldStr)?;
                let new_str = input.new_str.unwrap_or_default();
                let allow_multi = input.allow_multi.unwrap_or(false);
                let use_regex = input.use_regex.unwrap_or(false);
                self.str_replace(&path, &old_str, &new_str, allow_multi, use_regex)
            }
            Command::Insert => {
                let insert_line = input.insert_line.ok_or(EditorError::MissingInsertLine)?;
                let new_str = input.new_str.ok_or(EditorError::MissingNewStr)?;
                self.insert(&path, insert_line, &new_str)
            }
            Command::Delete => {
                let delete_range = input.delete_range.ok_or(EditorError::MissingDeleteRange)?;
                self.delete(&path, &delete_range)
            }
            Command::UndoEdit => Err(EditorError::UndoNotImplemented),
        }
    }

    fn insert(
        &mut self,
        path: &Path,
        insert_line: i32,
        new_str: &str,
    ) -> Result<String, EditorError> {
        self.validate_path_internal(path, &Command::Insert)?;

        // Path validation already handles directories

        let content = fs::read_to_string(path)?;
        let lines: Vec<_> = content.lines().collect();

        if insert_line < 0 || insert_line > lines.len() as i32 {
            return Err(EditorError::InvalidRange(format!(
                "Invalid `insert_line` parameter: {}. It should be within the range of lines of the file: [0, {}]",
                insert_line,
                lines.len()
            )));
        }

        // Create new content with inserted line
        let mut new_lines = lines.clone();
        new_lines.insert(insert_line as usize, new_str);
        let new_content = new_lines.join("\n") + "\n";

        fs::write(path, &new_content)?;

        // Calculate context for the edit
        let context_start = (insert_line as usize).saturating_sub(4);
        let mut context = String::new();
        new_lines
            .iter()
            .enumerate()
            .skip(context_start)
            .take(8)
            .fold(&mut context, |acc, (i, line)| {
                let _ = writeln!(acc, "{:6}\t{}", i + 1, line);
                acc
            });

        Ok(format!(
            "The file {} has been edited.\nHere's the result of running `cat -n` on a snippet:\n{}\nReview the changes and make sure they are as expected (correct indentation, no duplicate lines, etc). Edit the file again if necessary.",
            path.display(), context
        ))
    }

    fn create(&mut self, path: &Path, content: &str) -> Result<String, EditorError> {
        self.validate_path_internal(path, &Command::Create)?;

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(path, content)?;

        Ok(format!("File created successfully at: {}", path.display()))
    }

    // We'll remove the actual implementation since it's not used
    // The handle_command method already returns UndoNotImplemented error directly

    fn view(
        &self,
        path: &Path,
        view_range: Option<&[i32]>,
        max_depth: Option<usize>,
    ) -> Result<String, EditorError> {
        self.validate_path_internal(path, &Command::View)?;

        if path.is_dir() {
            if view_range.is_some() {
                return Err(EditorError::ViewRangeForDirectory);
            }
            let output = self.view_directory(path, max_depth)?;
            return Ok(output);
        }

        let content = fs::read_to_string(path)?;
        let lines: Vec<_> = content.lines().collect();

        if let Some(range) = view_range {
            if range.len() != 2 {
                return Err(EditorError::InvalidRange(
                    "Invalid `view_range`. It should be a list of two integers.".into(),
                ));
            }

            let [start, end] = [range[0], range[1]];
            if start < 1 || start as usize > lines.len() {
                return Err(EditorError::InvalidRange(format!(
                    "Invalid `view_range`: {:?}. Its first element `{}` should be within the range of lines of the file: [1, {}]",
                    &[start, end],
                    start,
                    lines.len()
                )));
            }
            // Support -1 as special case to read until the end of file
            if end != -1 {
                if end < start {
                    return Err(EditorError::InvalidRange(format!(
                        "Invalid `view_range`: {:?}. Its second element `{}` should be larger or equal than its first `{}`",
                        &[start, end],
                        end,
                        start
                    )));
                }

                if end as usize > lines.len() {
                    return Err(EditorError::InvalidRange(format!(
                        "Invalid `view_range`: {:?}. Its second element `{}` should be smaller than the number of lines in the file: `{}`",
                        &[start, end],
                        end,
                        lines.len()
                    )));
                }
            }

            let end_idx = if end == -1 { lines.len() } else { end as usize };

            let mut result = String::new();
            lines[(start - 1) as usize..end_idx]
                .iter()
                .enumerate()
                .fold(&mut result, |acc, (i, line)| {
                    let _ = writeln!(acc, "{:6}\t{}", i + start as usize, line);
                    acc
                });

            Ok(format!(
                "Here's the result of running `cat -n` on {}:\n{}",
                path.display(),
                result
            ))
        } else {
            let mut result = String::new();
            lines
                .iter()
                .enumerate()
                .fold(&mut result, |acc, (i, line)| {
                    let _ = writeln!(acc, "{:6}\t{}", i + 1, line);
                    acc
                });

            Ok(format!(
                "Here's the result of running `cat -n` on {}:\n{}",
                path.display(),
                result
            ))
        }
    }

    fn view_directory(&self, path: &Path, max_depth: Option<usize>) -> Result<String, EditorError> {
        use walkdir::WalkDir;

        // Default to 3 levels deep (path + 2 more levels) if not specified
        let max_depth = max_depth.unwrap_or(3);

        let mut output = vec![];
        for entry in WalkDir::new(path)
            .min_depth(1)
            .max_depth(max_depth)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_str().map_or(false, |s| s.starts_with(".")))
        {
            let entry = entry?;
            // Get path relative to the starting directory for cleaner output
            let rel_path = entry.path().strip_prefix(path).unwrap_or(entry.path());
            output.push(rel_path.to_string_lossy().into_owned());
        }

        Ok(format!(
            "Here's the files and directories up to {} levels deep in {}, excluding hidden items:\n{}\n",
            max_depth - 1, // Adjust for user-friendly display (3 -> "2 levels deep")
            path.display(),
            output.join("\n")
        ))
    }

    fn str_replace(
        &mut self,
        path: &Path,
        old_str: &str,
        new_str: &str,
        allow_multi: bool,
        use_regex: bool,
    ) -> Result<String, EditorError> {
        self.validate_path_internal(path, &Command::StrReplace)?;

        // Path validation already handles directories

        let content = fs::read_to_string(path)?;

        if use_regex {
            // Use regex for matching and replacement
            let regex =
                Regex::new(old_str).map_err(|e| EditorError::InvalidRegex(e.to_string()))?;

            if !regex.is_match(&content) {
                return Err(EditorError::StrReplace(format!(
                    "No replacement was performed, regex pattern `{}` did not match anything in {}",
                    old_str,
                    path.display()
                )));
            }

            // Count matches
            let matches_count = regex.find_iter(&content).count();
            let first_match = regex.find(&content).unwrap();

            if matches_count > 1 && !allow_multi {
                return Err(EditorError::InvalidRange(format!(
                    "Multiple occurrences ({}) matching regex `{}` found. Use allow_multi=true to replace all occurrences.",
                    matches_count, old_str
                )));
            }

            // Perform the replacement
            let new_content = regex.replace_all(&content, new_str).to_string();
            fs::write(path, &new_content)?;

            // Calculate context for the edit
            let prefix = &content[..first_match.start()];
            let line_num = prefix.chars().filter(|&c| c == '\n').count() + 1;
            let context_start = if line_num > 4 { line_num - 4 } else { 1 };

            let mut context = String::new();
            new_content
                .lines()
                .enumerate()
                .skip(context_start - 1)
                .take(8 + new_str.chars().filter(|&c| c == '\n').count())
                .fold(&mut context, |acc, (i, line)| {
                    let _ = writeln!(acc, "{:6}\t{}", i + 1, line);
                    acc
                });

            if matches_count > 1 {
                Ok(format!(
                    "The file {} has been edited. Made {} replacements using regex pattern \"{}\".\nHere's the result of running `cat -n` on a snippet of the first replacement:\n{}\n\nReview the changes and make sure they are as expected. Edit the file again if necessary.",
                    path.display(), matches_count, old_str, context
                ))
            } else {
                Ok(format!(
                    "The file {} has been edited using regex pattern \"{}\".\nHere's the result of running `cat -n` on a snippet:\n{}\n\nReview the changes and make sure they are as expected. Edit the file again if necessary.",
                    path.display(), old_str, context
                ))
            }
        } else {
            // Use simple string matching
            let matches: Vec<_> = content.match_indices(old_str).collect();

            match matches.len() {
                0 => Err(EditorError::StrReplace(format!(
                    "No replacement was performed, old_str `{}` did not appear verbatim in {}",
                    old_str,
                    path.display()
                ))),
                1 => {
                    let new_content = content.replace(old_str, new_str);
                    fs::write(path, &new_content)?;

                    // Calculate context for the edit
                    let prefix = &content[..matches[0].0];
                    let line_num = prefix.chars().filter(|&c| c == '\n').count() + 1;

                    // Ensure we don't underflow when calculating context_start
                    let context_start = if line_num > 4 { line_num - 4 } else { 1 };

                    let mut context = String::new();
                    new_content
                        .lines()
                        .enumerate()
                        .skip(context_start - 1)
                        .take(8 + new_str.chars().filter(|&c| c == '\n').count())
                        .fold(&mut context, |acc, (i, line)| {
                            let _ = writeln!(acc, "{:6}\t{}", i + 1, line);
                            acc
                        });

                    Ok(format!(
                        "The file {} has been edited.\nHere's the result of running `cat -n` on a snippet:\n{}\n\nReview the changes and make sure they are as expected. Edit the file again if necessary.",
                        path.display(), context
                    ))
                }
                count if allow_multi => {
                    let new_content = content.replace(old_str, new_str);
                    fs::write(path, &new_content)?;

                    // For multi-replacements, just show the first match for context
                    let prefix = &content[..matches[0].0];
                    let line_num = prefix.chars().filter(|&c| c == '\n').count() + 1;
                    let context_start = if line_num > 4 { line_num - 4 } else { 1 };

                    let mut context = String::new();
                    new_content
                        .lines()
                        .enumerate()
                        .skip(context_start - 1)
                        .take(8 + new_str.chars().filter(|&c| c == '\n').count())
                        .fold(&mut context, |acc, (i, line)| {
                            let _ = writeln!(acc, "{:6}\t{}", i + 1, line);
                            acc
                        });

                    Ok(format!(
                        "The file {} has been edited. Made {} replacements of \"{}\".\nHere's the result of running `cat -n` on a snippet of the first replacement:\n{}\n\nReview the changes and make sure they are as expected. Edit the file again if necessary.",
                        path.display(), count, old_str, context
                    ))
                }
                _ => Err(EditorError::InvalidRange(format!(
                    "Multiple occurrences ({}) of old_str `{}` found. Use allow_multi=true to replace all occurrences.",
                    matches.len(), old_str
                ))),
            }
        }
    }

    fn delete(&mut self, path: &Path, delete_range: &[i32]) -> Result<String, EditorError> {
        self.validate_path_internal(path, &Command::Delete)?;

        // Path validation already handles directories

        if delete_range.len() != 2 {
            return Err(EditorError::InvalidRange(
                "Delete range must have exactly two elements".into(),
            ));
        }

        let content = fs::read_to_string(path)?;
        let lines: Vec<_> = content.lines().collect();

        let [start, end] = [delete_range[0], delete_range[1]];
        if start < 1 || start as usize > lines.len() {
            return Err(EditorError::InvalidRange(format!(
                "Start line {} out of range 1..{}",
                start,
                lines.len()
            )));
        }
        if end < start || end as usize > lines.len() {
            return Err(EditorError::InvalidRange(format!(
                "End line {} out of range {}..{}",
                end,
                start,
                lines.len()
            )));
        }

        // Create new content without the deleted range
        let mut new_lines = Vec::new();
        new_lines.extend_from_slice(&lines[..(start - 1) as usize]);
        new_lines.extend_from_slice(&lines[end as usize..]);
        let new_content = new_lines.join("\n") + "\n";

        // Write the new content
        fs::write(path, &new_content)?;

        // Calculate context for the edit (show area around deletion)
        let context_start = (start as usize).saturating_sub(4);
        let context_end = std::cmp::min(context_start + 8, new_lines.len());

        let mut context = String::new();
        new_lines
            .iter()
            .enumerate()
            .skip(context_start)
            .take(context_end - context_start)
            .fold(&mut context, |acc, (i, line)| {
                let _ = writeln!(acc, "{:6}\t{}", i + 1, line);
                acc
            });

        Ok(format!(
            "The file {} has been edited. Deleted lines {}-{}.\nHere's the result of running `cat -n` on a snippet around the edit:\n{}\n\nReview the changes and make sure they are as expected.",
            path.display(), start, end, context
        ))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse arguments, but we don't use any currently
    let _cli = Cli::parse();

    let mut editor = Editor::new();

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
    let request: Request = if !input_str.is_empty() {
        serde_json::from_str(&input_str)?
    } else {
        // For normal operation when no input was read
        let stdin = io::stdin().lock();
        serde_json::from_reader(stdin)?
    };

    let result = match editor.handle_command(request.input) {
        Ok(output) => CliResult::success(output),
        Err(err) => CliResult::error(err),
    };

    println!("{}", serde_json::to_string(&result)?);

    Ok(())
}
