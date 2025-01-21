use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir;

#[derive(Debug, Error)]
enum EditorError {
    #[error("Path {0} does not exist")]
    PathNotFound(PathBuf),
    #[error("Path {0} is not an absolute path")]
    NotAbsolutePath(PathBuf),
    #[error("Invalid view range: {0}")]
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
            _ => Err(EditorError::InvalidRange(format!(
                "Unknown command: {}",
                input.command
            ))),
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

                let context_start = line_num.saturating_sub(4);
                let context: String = new_content
                    .lines()
                    .skip(context_start - 1)
                    .take(8 + new_str.chars().filter(|&c| c == '\n').count())
                    .enumerate()
                    .map(|(i, line)| format!("{:6}\t{}", i + context_start, line))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use tempfile::{tempdir, NamedTempFile};

    fn create_test_input(command: &str, path: &str) -> Request {
        Request {
            input: Input {
                command: command.to_string(),
                path: path.to_string(),
                view_range: None,
                old_str: None,
                new_str: None,
            },
        }
    }

    fn create_test_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "{}", content).unwrap();
        file
    }

    #[test]
    fn test_view_existing_file() {
        let mut editor = Editor::new();
        let file = create_test_file("File content");

        let input = create_test_input("view", file.path().to_str().unwrap());
        let result = editor.handle_command(input.input).unwrap();

        assert!(result.contains("File content"));
        assert!(result.contains("1")); // Line number
    }

    #[test]
    fn test_view_directory() {
        let mut editor = Editor::new();
        let dir = tempdir().unwrap();

        // Create test files in directory
        let file1_path = dir.path().join("file1.txt");
        let file2_path = dir.path().join("file2.txt");
        File::create(&file1_path).unwrap();
        File::create(&file2_path).unwrap();

        let input = create_test_input("view", dir.path().to_str().unwrap());
        let result = editor.handle_command(input.input).unwrap();

        assert!(result.contains("file1.txt"));
        assert!(result.contains("file2.txt"));
    }

    #[test]
    fn test_view_file_with_range() {
        let mut editor = Editor::new();
        let file = create_test_file("Line 1\nLine 2\nLine 3\nLine 4");

        let mut input = create_test_input("view", file.path().to_str().unwrap());
        input.input.view_range = Some(vec![2, 3]);

        let result = editor.handle_command(input.input).unwrap();

        assert!(result.contains("Line 2"));
        assert!(result.contains("Line 3"));
        assert!(!result.contains("Line 1"));
        assert!(!result.contains("Line 4"));
    }

    #[test]
    fn test_view_file_invalid_range() {
        let mut editor = Editor::new();
        let file = create_test_file("Line 1\nLine 2\nLine 3\nLine 4");

        let mut input = create_test_input("view", file.path().to_str().unwrap());
        input.input.view_range = Some(vec![3, 2]); // end before start

        let result = editor.handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_))));
    }

    #[test]
    fn test_view_nonexistent_file() {
        let mut editor = Editor::new();
        let input = create_test_input("view", "/nonexistent/file.txt");

        let result = editor.handle_command(input.input);
        assert!(matches!(result, Err(EditorError::PathNotFound(_))));
    }

    #[test]
    fn test_view_directory_with_range() {
        let mut editor = Editor::new();
        let dir = tempdir().unwrap();

        let mut input = create_test_input("view", dir.path().to_str().unwrap());
        input.input.view_range = Some(vec![1, 2]);

        let result = editor.handle_command(input.input);
        assert!(matches!(result, Err(EditorError::ViewRangeForDirectory)));
    }

    #[test]
    fn test_str_replace_unique() {
        let mut editor = Editor::new();
        let file = create_test_file("Original content");

        let mut input = create_test_input("str_replace", file.path().to_str().unwrap());
        input.input.old_str = Some("Original".to_string());
        input.input.new_str = Some("New".to_string());

        let result = editor.handle_command(input.input).unwrap();

        assert!(result.contains("has been edited"));
        let content = fs::read_to_string(file.path()).unwrap();
        assert_eq!(content.trim(), "New content");
    }

    #[test]
    fn test_str_replace_nonexistent() {
        let mut editor = Editor::new();
        let file = create_test_file("Original content");

        let mut input = create_test_input("str_replace", file.path().to_str().unwrap());
        input.input.old_str = Some("Nonexistent".to_string());
        input.input.new_str = Some("New".to_string());

        let result = editor.handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_))));
    }

    #[test]
    fn test_str_replace_multiple_occurrences() {
        let mut editor = Editor::new();
        let file = create_test_file("Test test test");

        let mut input = create_test_input("str_replace", file.path().to_str().unwrap());
        input.input.old_str = Some("test".to_string());
        input.input.new_str = Some("example".to_string());

        let result = editor.handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_))));
    }

    #[test]
    fn test_str_replace_history() {
        let mut editor = Editor::new();
        let file = create_test_file("Original content");
        let path = file.path().to_path_buf();

        let mut input = create_test_input("str_replace", path.to_str().unwrap());
        input.input.old_str = Some("Original".to_string());
        input.input.new_str = Some("New".to_string());

        editor.handle_command(input.input).unwrap();

        assert_eq!(editor.history.get(&path).unwrap()[0], "Original content\n");
    }

    // Insert command tests

    #[test]
    fn test_insert_middle() {
        let mut editor = Editor::new();
        let file = create_test_file("Line 1\nLine 2\nLine 3");

        let mut input = create_test_input("insert", file.path().to_str().unwrap());
        input.input.insert_line = Some(2);
        input.input.new_str = Some("New Line".to_string());

        let result = editor.handle_command(input.input).unwrap();
        assert!(result.contains("has been edited"));

        let content = fs::read_to_string(file.path()).unwrap();
        assert_eq!(content.trim(), "Line 1\nLine 2\nNew Line\nLine 3");
    }

    #[test]
    fn test_insert_beginning() {
        let mut editor = Editor::new();
        let file = create_test_file("Line 1\nLine 2");

        let mut input = create_test_input("insert", file.path().to_str().unwrap());
        input.input.insert_line = Some(0);
        input.input.new_str = Some("New First Line".to_string());

        let result = editor.handle_command(input.input).unwrap();
        assert!(result.contains("has been edited"));

        let content = fs::read_to_string(file.path()).unwrap();
        assert_eq!(content.trim(), "New First Line\nLine 1\nLine 2");
    }

    #[test]
    fn test_insert_end() {
        let mut editor = Editor::new();
        let file = create_test_file("Line 1\nLine 2");

        let mut input = create_test_input("insert", file.path().to_str().unwrap());
        input.input.insert_line = Some(2);
        input.input.new_str = Some("New Last Line".to_string());

        let result = editor.handle_command(input.input).unwrap();
        assert!(result.contains("has been edited"));

        let content = fs::read_to_string(file.path()).unwrap();
        assert_eq!(content.trim(), "Line 1\nLine 2\nNew Last Line");
    }

    #[test]
    fn test_insert_invalid_line() {
        let mut editor = Editor::new();
        let file = create_test_file("Line 1\nLine 2");

        let mut input = create_test_input("insert", file.path().to_str().unwrap());
        input.input.insert_line = Some(5);
        input.input.new_str = Some("Invalid Line".to_string());

        let result = editor.handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_))));
    }

    #[test]
    fn test_undo_str_replace() {
        let mut editor = Editor::new();
        let file = create_test_file("Original content");
        let path = file.path().to_str().unwrap();

        // First do a str_replace
        let mut input = create_test_input("str_replace", path);
        input.input.old_str = Some("Original".to_string());
        input.input.new_str = Some("New".to_string());
        editor.handle_command(input.input).unwrap();

        // Then undo it
        let input = create_test_input("undo_edit", path);
        let result = editor.handle_command(input.input).unwrap();

        assert!(result.contains("undone successfully"));
        let content = fs::read_to_string(file.path()).unwrap();
        assert_eq!(content.trim(), "Original content");
    }

    #[test]
    fn test_undo_no_history() {
        let mut editor = Editor::new();
        let file = create_test_file("");
        let input = create_test_input("undo_edit", file.path().to_str().unwrap());

        let result = editor.handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_)))); // or a specific NoHistory error
    }

    #[test]
    fn test_path_validation() {
        let editor = Editor::new();

        // Test relative path
        assert!(matches!(
            editor.validate_path(Path::new("relative/path.txt"), false),
            Err(EditorError::NotAbsolutePath(_))
        ));

        // Test non-existent path
        assert!(matches!(
            editor.validate_path(Path::new("/nonexistent/file.txt"), false),
            Err(EditorError::PathNotFound(_))
        ));
    }
}
