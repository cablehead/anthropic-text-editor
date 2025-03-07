use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[cfg(test)]
mod tests;

#[derive(Debug, Error)]
enum EditorError {
    #[error("Path {0} does not exist")]
    PathNotFound(PathBuf),
    #[error("Path {0} is not an absolute path")]
    NotAbsolutePath(PathBuf),
    #[error("Invalid range: {0}")]
    InvalidRange(String),
    #[error("View range not allowed for directory")]
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
    #[error("Parameter `delete_range` is required for command: delete")]
    MissingDeleteRange,
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Walk error: {0}")]
    Walk(#[from] walkdir::Error),
}

#[derive(Debug, Deserialize)]
struct Input {
    command: String,
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

    fn validate_path(&self, path: &Path, command: &str) -> Result<(), EditorError> {
        // Check if it's an absolute path
        if !path.is_absolute() {
            return Err(EditorError::NotAbsolutePath(path.to_path_buf()));
        }

        // For create, file should not exist
        if command == "create" {
            if path.exists() {
                return Err(EditorError::FileAlreadyExists(path.to_path_buf()));
            }
        } else {
            // For other commands, file should exist
            if !path.exists() {
                return Err(EditorError::PathNotFound(path.to_path_buf()));
            }

            // Check if directory for non-view command
            if path.is_dir() && command != "view" {
                return Err(EditorError::InvalidRange(
                    format!("The path {} is a directory and only the `view` command can be used on directories", path.display())
                ));
            }
        }

        Ok(())
    }

    fn handle_command(&mut self, input: Input) -> Result<String, EditorError> {
        let path = PathBuf::from(&input.path);

        match input.command.as_str() {
            "view" => self.view(&path, input.view_range.as_deref(), input.max_depth),
            "create" => {
                let file_text = input
                    .file_text
                    .ok_or_else(|| EditorError::MissingFileText)?;
                self.create(&path, &file_text)
            }
            "str_replace" => {
                let old_str = input
                    .old_str
                    .ok_or_else(|| EditorError::StrReplace("Missing old_str".into()))?;
                let new_str = input.new_str.unwrap_or_default();
                let allow_multi = input.allow_multi.unwrap_or(false);
                self.str_replace(&path, &old_str, &new_str, allow_multi)
            }
            "insert" => {
                let insert_line = input
                    .insert_line
                    .ok_or_else(|| EditorError::InvalidRange("Missing insert_line".into()))?;
                let new_str = input
                    .new_str
                    .ok_or_else(|| EditorError::InvalidRange("Missing new_str".into()))?;
                self.insert(&path, insert_line, &new_str)
            }
            "delete" => {
                let delete_range = input
                    .delete_range
                    .ok_or_else(|| EditorError::MissingDeleteRange)?;
                self.delete(&path, &delete_range)
            }
            "undo_edit" => Err(EditorError::UndoNotImplemented),
            _ => Err(EditorError::InvalidRange(format!(
                "Unknown command: {}",
                input.command
            ))),
        }
    }

    fn insert(
        &mut self,
        path: &Path,
        insert_line: i32,
        new_str: &str,
    ) -> Result<String, EditorError> {
        self.validate_path(path, "insert")?;

        if path.is_dir() {
            return Err(EditorError::InvalidRange(
                "Cannot perform insert on directory".into(),
            ));
        }

        let content = fs::read_to_string(path)?;
        let lines: Vec<_> = content.lines().collect();

        if insert_line < 0 || insert_line > lines.len() as i32 {
            return Err(EditorError::InvalidRange(format!(
                "Invalid insert_line parameter: {}. It should be within the range of lines of the file: [0, {}]",
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
        self.validate_path(path, "create")?;

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
        self.validate_path(path, "view")?;

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
                    "Range must have exactly two elements".into(),
                ));
            }

            let [start, end] = [range[0], range[1]];
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

            let mut result = String::new();
            lines[(start - 1) as usize..end as usize]
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
    ) -> Result<String, EditorError> {
        self.validate_path(path, "str_replace")?;

        if path.is_dir() {
            return Err(EditorError::InvalidRange(
                "Cannot perform str_replace on directory".into(),
            ));
        }

        let content = fs::read_to_string(path)?;
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

    fn delete(&mut self, path: &Path, delete_range: &[i32]) -> Result<String, EditorError> {
        self.validate_path(path, "delete")?;

        if path.is_dir() {
            return Err(EditorError::InvalidRange(
                "Cannot perform delete on directory".into(),
            ));
        }

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
    let stdin = io::stdin().lock();
    let request: Request = serde_json::from_reader(stdin)?;

    let result = match editor.handle_command(request.input) {
        Ok(output) => CliResult::success(output),
        Err(err) => CliResult::error(err),
    };

    println!("{}", serde_json::to_string(&result)?);

    Ok(())
}
