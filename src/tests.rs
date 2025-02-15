#[cfg(test)]
use crate::*;
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
            insert_line: None, // Added this field
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
    input.input.old_str = Some("Nonexistent".into());
    input.input.new_str = Some("New".into());

    let result = CliResult::error(editor.handle_command(input.input).unwrap_err());

    let json = serde_json::to_string(&result).unwrap();
    assert_eq!(
        json,
        format!(
            r#"{{"error":"No replacement was performed, old_str `Nonexistent` did not appear verbatim in {}"}}"#,
            file.path().display()
        )
    );
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
