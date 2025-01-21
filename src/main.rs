use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir;

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
    old_str: Option<String>,
    #[serde(default)]
    new_str: Option<String>,
    #[serde(default)]
    insert_line: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct Request {
    input: Input,
}

#[derive(Debug, Serialize)]
struct CliResult {
    output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl CliResult {
    fn success(output: String) -> Self {
        Self {
            output,
            error: None,
        }
    }

    fn error(err: impl std::error::Error) -> Self {
        Self {
            output: String::new(),
            error: Some(err.to_string()),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    json: bool,
}

struct Editor {
    history: im::HashMap<PathBuf, Vec<String>>,
}

impl Editor {
    fn new() -> Self {
        Self {
            history: im::HashMap::new(),
        }
    }

    fn validate_path(&self, path: &Path, allow_missing: bool) -> Result<(), EditorError> {
        if !path.is_absolute() {
            return Err(EditorError::NotAbsolutePath(path.to_path_buf()));
        }

        if !allow_missing && !path.exists() {
            return Err(EditorError::PathNotFound(path.to_path_buf()));
        }

        Ok(())
    }

    fn handle_command(&mut self, input: Input) -> Result<String, EditorError> {
        let path = PathBuf::from(&input.path);

        match input.command.as_str() {
            "view" => self.view(&path, input.view_range.as_deref()),
            "str_replace" => {
                let old_str = input
                    .old_str
                    .ok_or_else(|| EditorError::InvalidRange("Missing old_str".into()))?;
                let new_str = input.new_str.unwrap_or_default();
                self.str_replace(&path, &old_str, &new_str)
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
            "undo_edit" => self.undo_edit(&path),
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
        self.validate_path(path, false)?;

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

        // Save current content to history
        self.history
            .entry(path.to_path_buf())
            .or_insert_with(Vec::new)
            .push(content.clone());

        // Create new content with inserted line
        let mut new_lines = lines.clone();
        new_lines.insert(insert_line as usize, new_str);
        let new_content = new_lines.join("\n") + "\n";

        fs::write(path, &new_content)?;

        // Calculate context for the edit
        let context_start = (insert_line as usize).saturating_sub(4);
        let context: String = new_lines
            .iter()
            .skip(context_start)
            .take(8)
            .enumerate()
            .map(|(i, line)| format!("{:6}\t{}", i + context_start + 1, line))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            "The file {} has been edited.\nHere's the result of running `cat -n` on a snippet:\n{}\n\nReview the changes and make sure they are as expected (correct indentation, no duplicate lines, etc). Edit the file again if necessary.",
            path.display(), context
        ))
    }

    fn undo_edit(&mut self, path: &Path) -> Result<String, EditorError> {
        self.validate_path(path, false)?;

        let history = self.history.get_mut(path).ok_or_else(|| {
            EditorError::InvalidRange(format!("No edit history found for {}", path.display()))
        })?;

        if let Some(previous_content) = history.pop() {
            fs::write(path, &previous_content)?;

            Ok(format!(
                "Last edit to {} undone successfully.\nHere's the result of running `cat -n`:\n{}\n",
                path.display(),
                previous_content.lines()
                    .enumerate()
                    .map(|(i, line)| format!("{:6}\t{}\n", i + 1, line))
                    .collect::<String>()
            ))
        } else {
            Err(EditorError::InvalidRange(format!(
                "No more history available for {}",
                path.display()
            )))
        }
    }

    fn view(&self, path: &Path, view_range: Option<&[i32]>) -> Result<String, EditorError> {
        self.validate_path(path, false)?;

        if path.is_dir() {
            if view_range.is_some() {
                return Err(EditorError::ViewRangeForDirectory);
            }
            let output = self.view_directory(path)?;
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

            let result: String = lines[(start - 1) as usize..end as usize]
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:6}\t{}\n", i + start as usize, line))
                .collect();
            Ok(format!(
                "Here's the result of running `cat -n` on {}:\n{}",
                path.display(),
                result
            ))
        } else {
            let result: String = lines
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:6}\t{}\n", i + 1, line))
                .collect();
            Ok(format!(
                "Here's the result of running `cat -n` on {}:\n{}",
                path.display(),
                result
            ))
        }
    }

    fn view_directory(&self, path: &Path) -> Result<String, EditorError> {
        use walkdir::WalkDir;

        let mut output = vec![];
        for entry in WalkDir::new(path)
            .min_depth(1)
            .max_depth(2)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_str().map_or(false, |s| s.starts_with(".")))
        {
            let entry = entry?;
            output.push(entry.path().to_string_lossy().into_owned());
        }

        Ok(format!("Here's the files and directories up to 2 levels deep in {}, excluding hidden items:\n{}\n",
            path.display(),
            output.join("\n")
        ))
    }

    fn str_replace(
        &mut self,
        path: &Path,
        old_str: &str,
        new_str: &str,
    ) -> Result<String, EditorError> {
        self.validate_path(path, false)?;

        if path.is_dir() {
            return Err(EditorError::InvalidRange(
                "Cannot perform str_replace on directory".into(),
            ));
        }

        let content = fs::read_to_string(path)?;
        let matches: Vec<_> = content.match_indices(old_str).collect();

        match matches.len() {
            0 => Err(EditorError::InvalidRange(format!(
                "No replacement was performed, old_str `{}` did not appear verbatim in {}",
                old_str,
                path.display()
            ))),
            1 => {
                // Save current content to history
                self.history
                    .entry(path.to_path_buf())
                    .or_insert_with(Vec::new)
                    .push(content.clone());

                let new_content = content.replace(old_str, new_str);
                fs::write(path, &new_content)?;

                // Calculate context for the edit
                let prefix = &content[..matches[0].0];
                let line_num = prefix.chars().filter(|&c| c == '\n').count() + 1;

                // Ensure we don't underflow when calculating context_start
                let context_start = if line_num > 4 { line_num - 4 } else { 1 };

                let context: String = new_content
                    .lines()
                    .enumerate()
                    .skip(context_start - 1)
                    .take(8 + new_str.chars().filter(|&c| c == '\n').count())
                    .map(|(i, line)| format!("{:6}\t{}", i + 1, line))
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(format!(
                "The file {} has been edited.\nHere's the result of running `cat -n` on a snippet:\n{}\n\nReview the changes and make sure they are as expected. Edit the file again if necessary.",
                path.display(), context
            ))
            }
            _ => Err(EditorError::InvalidRange(format!(
                "Multiple occurrences of old_str `{}` found. Please ensure it is unique",
                old_str
            ))),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.json {
        let mut editor = Editor::new();
        let stdin = io::stdin().lock();
        let request: Request = serde_json::from_reader(stdin)?;

        let result = match editor.handle_command(request.input) {
            Ok(output) => CliResult::success(output),
            Err(err) => CliResult::error(err),
        };

        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!("Please run with --json flag for JSON protocol mode");
    }

    Ok(())
}
