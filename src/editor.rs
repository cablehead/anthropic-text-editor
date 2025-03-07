use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

/// Commands supported by the editor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
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
pub enum EditorError {
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

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Deserialize)]
pub struct Input {
    #[serde(deserialize_with = "deserialize_command")]
    pub command: Command,
    pub path: String,
    #[serde(default)]
    pub view_range: Option<Vec<i32>>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub old_str: Option<String>,
    #[serde(default)]
    pub new_str: Option<String>,
    #[serde(default)]
    pub insert_line: Option<i32>,
    #[serde(default)]
    pub file_text: Option<String>,
    #[serde(default)]
    pub delete_range: Option<Vec<i32>>,
    #[serde(default)]
    pub allow_multi: Option<bool>,
    #[serde(default)]
    pub use_regex: Option<bool>,
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
pub struct Request {
    pub input: Input,
}

#[derive(Debug, Serialize)]
pub struct CliResult {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl CliResult {
    pub fn success(content: String) -> Self {
        Self {
            content,
            is_error: None,
        }
    }

    pub fn error(err: EditorError) -> Self {
        Self {
            content: err.to_string(),
            is_error: Some(true),
        }
    }
}

// Method that validates paths based on command type
pub fn validate_path(path: &Path, command: &Command) -> Result<(), EditorError> {
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

pub fn handle_command(input: Input) -> Result<String, EditorError> {
    let path = PathBuf::from(&input.path);

    match input.command {
        Command::View => view(&path, input.view_range.as_deref(), input.max_depth),
        Command::Create => {
            let file_text = input.file_text.ok_or(EditorError::MissingFileText)?;
            create(&path, &file_text)
        }
        Command::StrReplace => {
            let old_str = input.old_str.ok_or(EditorError::MissingOldStr)?;
            let new_str = input.new_str.unwrap_or_default();
            let allow_multi = input.allow_multi.unwrap_or(false);
            let use_regex = input.use_regex.unwrap_or(false);
            str_replace(&path, &old_str, &new_str, allow_multi, use_regex)
        }
        Command::Insert => {
            let insert_line = input.insert_line.ok_or(EditorError::MissingInsertLine)?;
            let new_str = input.new_str.ok_or(EditorError::MissingNewStr)?;
            insert(&path, insert_line, &new_str)
        }
        Command::Delete => {
            let delete_range = input.delete_range.ok_or(EditorError::MissingDeleteRange)?;
            delete(&path, &delete_range)
        }
        Command::UndoEdit => Err(EditorError::UndoNotImplemented),
    }
}

pub fn insert(path: &Path, insert_line: i32, new_str: &str) -> Result<String, EditorError> {
    validate_path(path, &Command::Insert)?;

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

pub fn create(path: &Path, content: &str) -> Result<String, EditorError> {
    validate_path(path, &Command::Create)?;

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

pub fn view(
    path: &Path,
    view_range: Option<&[i32]>,
    max_depth: Option<usize>,
) -> Result<String, EditorError> {
    validate_path(path, &Command::View)?;

    if path.is_dir() {
        // Handle directory listing
        if view_range.is_some() {
            return Err(EditorError::ViewRangeForDirectory);
        }

        let mut files = Vec::new();
        let depth = max_depth.unwrap_or(1);
        list_files_recursive(path, &mut files, 0, depth)?;
        files.sort();

        Ok(files.join("\n"))
    } else {
        // Handle file content view
        let content = fs::read_to_string(path)?;
        let lines: Vec<_> = content.lines().collect();

        if let Some(range) = view_range {
            if range.len() != 2 {
                return Err(EditorError::InvalidRange(
                    "view_range must be an array with exactly 2 elements: [start_line, end_line]"
                        .to_string(),
                ));
            }

            let start = range[0];
            let end = range[1];

            // Adjust negative indices
            let adjusted_start = if start < 0 {
                (lines.len() as i32 + start).max(0)
            } else {
                start
            };
            let adjusted_end = if end < 0 {
                (lines.len() as i32 + end).max(0)
            } else {
                end
            };

            if adjusted_start > adjusted_end {
                return Err(EditorError::InvalidRange(format!(
                    "view_range start {} must be <= end {}",
                    start, end
                )));
            }

            if adjusted_start >= lines.len() as i32 {
                return Err(EditorError::InvalidRange(format!(
                    "Invalid view_range: {}. The file has {} lines.",
                    start,
                    lines.len()
                )));
            }

            // Get the specified range, clamping end to the actual line count
            let end_idx = (adjusted_end as usize + 1).min(lines.len());
            let sliced_lines = &lines[adjusted_start as usize..end_idx];
            Ok(sliced_lines.join("\n"))
        } else {
            // Return the whole file content
            Ok(content.trim_end().to_string())
        }
    }
}

fn list_files_recursive(
    dir: &Path,
    files: &mut Vec<String>,
    depth: usize,
    max_depth: usize,
) -> std::io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip hidden files
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map_or(false, |name| name.starts_with('.'))
        {
            continue;
        }

        files.push(path.to_string_lossy().to_string());

        if path.is_dir() && depth < max_depth {
            list_files_recursive(&path, files, depth + 1, max_depth)?;
        }
    }

    Ok(())
}

pub fn str_replace(
    path: &Path,
    old_str: &str,
    new_str: &str,
    allow_multi: bool,
    use_regex: bool,
) -> Result<String, EditorError> {
    validate_path(path, &Command::StrReplace)?;

    let content = fs::read_to_string(path)?;

    let (new_content, count) = if use_regex {
        // Regex-based replacement
        let re = Regex::new(old_str)
            .map_err(|e| EditorError::InvalidRegex(format!("Invalid regex pattern: {}", e)))?;

        if !allow_multi {
            // Check for multiple matches first
            let matches: Vec<_> = re.find_iter(&content).collect();
            if matches.len() > 1 {
                return Err(EditorError::StrReplace(
                    format!("The regex pattern matches in multiple places ({} matches). Use `allow_multi: true` if you want to replace all occurrences.", matches.len())
                ));
            } else if matches.is_empty() {
                return Err(EditorError::StrReplace(
                    "The regex pattern does not match anywhere in the file.".to_string(),
                ));
            }
        }

        let new_content = re.replace_all(&content, new_str).to_string();
        let count = re.find_iter(&content).count();
        (new_content, count)
    } else {
        // Literal string replacement
        if !content.contains(old_str) {
            return Err(EditorError::StrReplace(
                "The string was not found in the file.".to_string(),
            ));
        }

        if !allow_multi {
            // Count occurrences to check if there are multiple matches
            let count = content.matches(old_str).count();
            if count > 1 {
                return Err(EditorError::StrReplace(
                    format!("The string occurs in multiple places ({} occurrences). Use `allow_multi: true` if you want to replace all occurrences.", count)
                ));
            }
        }

        let new_content = content.replace(old_str, new_str);
        let count = content.matches(old_str).count();
        (new_content, count)
    };

    fs::write(path, &new_content)?;

    Ok(format!(
        "The file {} has been edited. Replaced {} occurrences of '{}'.",
        path.display(),
        count,
        old_str
    ))
}

pub fn delete(path: &Path, delete_range: &[i32]) -> Result<String, EditorError> {
    validate_path(path, &Command::Delete)?;

    if delete_range.len() != 2 {
        return Err(EditorError::InvalidRange(
            "delete_range must be an array with exactly 2 elements: [start_line, end_line]"
                .to_string(),
        ));
    }

    let content = fs::read_to_string(path)?;
    let lines: Vec<_> = content.lines().collect();

    let start = delete_range[0];
    let end = delete_range[1];

    // Validate range
    if start < 1 || end < 1 || start > end || start > lines.len() as i32 {
        return Err(EditorError::InvalidRange(format!(
            "Invalid delete_range: [{}, {}]. Line numbers should be within the range of lines in the file (1-{}) and start <= end.",
            start, end, lines.len()
        )));
    }

    // Adjust to 0-based indexing
    let start_idx = start as usize - 1;
    let end_idx = (end as usize).min(lines.len());

    // Create new content excluding deleted lines
    let mut new_lines = Vec::new();
    new_lines.extend_from_slice(&lines[0..start_idx]);
    new_lines.extend_from_slice(&lines[end_idx..]);

    let new_content = new_lines.join("\n") + "\n";
    fs::write(path, &new_content)?;

    Ok(format!(
        "Deleted lines {}-{} from the file {}",
        start,
        end,
        path.display()
    ))
}
