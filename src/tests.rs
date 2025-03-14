#[cfg(test)]
use crate::editor::*;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::str::FromStr;
use tempfile::{tempdir, NamedTempFile};

mod test_helpers {
    use super::*;

    // Helper for creating test inputs
    pub fn create_test_input(command: &str, path: &str) -> Request {
        Request {
            input: Input {
                command: Command::from_str(command).unwrap_or(Command::View),
                path: path.to_string(),
                view_range: None,
                max_depth: None,
                old_str: None,
                new_str: None,
                insert_line: None,
                file_text: None,
                delete_range: None,
                allow_multi: None,
                use_regex: None,
            },
        }
    }

    // Helper for creating test files with content
    pub fn create_test_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "{}", content).unwrap();
        file
    }

    // Helper to verify file content
    pub fn verify_file_content(file_path: &Path, expected: &str) {
        let content = fs::read_to_string(file_path).unwrap();
        assert_eq!(content.trim(), expected);
    }

    // Helper to check for success result
    pub fn assert_success_contains(result: &str, expected_text: &str) {
        assert!(
            result.contains(expected_text),
            "Expected result to contain '{}', but got: {}",
            expected_text,
            result
        );
    }

    // Helper to verify edit operations
    pub fn verify_edit_operation(result: &str, file_path: &Path, expected_content: &str) {
        assert_success_contains(result, "has been edited");
        verify_file_content(file_path, expected_content);
    }
}

use test_helpers::*;

mod view_tests {
    use super::*;

    #[test]
    fn test_view_existing_file() {
        let file = create_test_file("File content");

        let input = create_test_input("view", file.path().to_str().unwrap());
        let result = handle_command(input.input).unwrap();

        assert_success_contains(&result, "File content");
        assert_success_contains(&result, "Here's the result of running `cat -n`");
        assert_success_contains(&result, "     1\tFile content");
    }

    #[test]
    fn test_view_directory() {
        let dir = tempdir().unwrap();

        // Create test files in directory
        let file1_path = dir.path().join("file1.txt");
        let file2_path = dir.path().join("file2.txt");
        File::create(&file1_path).unwrap();
        File::create(&file2_path).unwrap();

        let input = create_test_input("view", dir.path().to_str().unwrap());
        let result = handle_command(input.input).unwrap();

        assert_success_contains(&result, "file1.txt");
        assert_success_contains(&result, "file2.txt");
    }

    #[test]
    fn test_view_directory_custom_depth() {
        let dir = tempdir().unwrap();

        // Create test files with nested directories
        let subdir_path = dir.path().join("subdir");
        fs::create_dir(&subdir_path).unwrap();
        let nested_path = subdir_path.join("nested");
        fs::create_dir(&nested_path).unwrap();
        let deep_path = nested_path.join("deep");
        fs::create_dir(&deep_path).unwrap();

        // Create file at each level
        File::create(dir.path().join("root.txt")).unwrap();
        File::create(subdir_path.join("level1.txt")).unwrap();
        File::create(nested_path.join("level2.txt")).unwrap();
        File::create(deep_path.join("level3.txt")).unwrap();

        // Test with default depth
        let input = create_test_input("view", dir.path().to_str().unwrap());
        let result = handle_command(input.input).unwrap();

        // Default depth is 1
        assert_success_contains(&result, "root.txt");
        assert_success_contains(&result, "subdir");
        assert!(!result.contains("level2.txt"));
        assert!(!result.contains("level3.txt"));

        // Test with increased depth
        let mut deep_input = create_test_input("view", dir.path().to_str().unwrap());
        deep_input.input.max_depth = Some(4);
        let deep_result = handle_command(deep_input.input).unwrap();

        // Should now show all files including level3
        assert_success_contains(&deep_result, "root.txt");
        assert_success_contains(&deep_result, "level1.txt");
        assert_success_contains(&deep_result, "level2.txt");
        assert_success_contains(&deep_result, "level3.txt");
    }

    #[test]
    fn test_view_file_with_range() {
        let file = create_test_file("Line 1\nLine 2\nLine 3\nLine 4");

        let mut input = create_test_input("view", file.path().to_str().unwrap());
        input.input.view_range = Some(vec![1, 2]); // 0-based indexing becomes 1-based

        let result = handle_command(input.input).unwrap();

        assert_success_contains(&result, "Line 2");
        assert_success_contains(&result, "Line 3");
        assert!(!result.contains("Line 1"));
        assert!(!result.contains("Line 4"));

        // Check for line numbers
        assert_success_contains(&result, "     2\tLine 2");
        assert_success_contains(&result, "     3\tLine 3");
        assert_success_contains(&result, "Here's the result of running `cat -n`");
    }

    #[test]
    fn test_view_file_with_negative_end() {
        let file = create_test_file("Line 1\nLine 2\nLine 3\nLine 4");

        let mut input = create_test_input("view", file.path().to_str().unwrap());
        input.input.view_range = Some(vec![1, -1]); // Adjusted for 0-based indexing

        let result = handle_command(input.input).unwrap();

        assert_success_contains(&result, "Line 2");
        assert_success_contains(&result, "Line 3");
        assert_success_contains(&result, "Line 4");
        assert!(!result.contains("Line 1"));

        // Check for line numbers
        assert_success_contains(&result, "     2\tLine 2");
        assert_success_contains(&result, "     3\tLine 3");
        assert_success_contains(&result, "     4\tLine 4");
        assert_success_contains(&result, "Here's the result of running `cat -n`");
    }

    #[test]
    fn test_view_file_invalid_range() {
        let file = create_test_file("Line 1\nLine 2\nLine 3\nLine 4");

        let mut input = create_test_input("view", file.path().to_str().unwrap());
        input.input.view_range = Some(vec![3, 2]); // end before start

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_))));
    }

    #[test]
    fn test_view_nonexistent_file() {
        let input = create_test_input("view", "/nonexistent/file.txt");

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::PathNotFound(_))));
    }

    #[test]
    fn test_view_directory_with_range() {
        let dir = tempdir().unwrap();

        let mut input = create_test_input("view", dir.path().to_str().unwrap());
        input.input.view_range = Some(vec![1, 2]);

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::ViewRangeForDirectory)));
    }
}

mod str_replace_tests {
    use super::*;

    #[test]
    fn test_str_replace_unique() {
        let file = create_test_file("Original content");
        let path = file.path();

        let mut input = create_test_input("str_replace", path.to_str().unwrap());
        input.input.old_str = Some("Original".to_string());
        input.input.new_str = Some("New".to_string());

        let result = handle_command(input.input).unwrap();
        verify_edit_operation(&result, path, "New content");
    }

    #[test]
    fn test_str_replace_nonexistent() {
        let file = create_test_file("Original content");
        let mut input = create_test_input("str_replace", file.path().to_str().unwrap());
        input.input.old_str = Some("Nonexistent".into());
        input.input.new_str = Some("New".into());

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::StrReplace(_))));

        if let Err(EditorError::StrReplace(msg)) = result {
            assert!(msg.contains("not found"));
        }
    }

    #[test]
    fn test_str_replace_multiple_occurrences() {
        let file = create_test_file("Test test test");

        let mut input = create_test_input("str_replace", file.path().to_str().unwrap());
        input.input.old_str = Some("test".to_string());
        input.input.new_str = Some("example".to_string());

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::StrReplace(_))));
    }

    #[test]
    fn test_str_replace_multiple_allowed() {
        let file = create_test_file("Test test test");
        let path = file.path();

        let mut input = create_test_input("str_replace", path.to_str().unwrap());
        input.input.old_str = Some("test".to_string());
        input.input.new_str = Some("example".to_string());
        input.input.allow_multi = Some(true);

        let result = handle_command(input.input).unwrap();
        assert_success_contains(&result, "Replaced 2 occurrences");
        verify_file_content(path, "Test example example");
    }

    #[test]
    fn test_str_replace_with_regex() {
        let file = create_test_file("Test123");
        let path = file.path();

        let mut input = create_test_input("str_replace", path.to_str().unwrap());
        input.input.old_str = Some(r"Test\d+".to_string());
        input.input.new_str = Some("Example".to_string());
        input.input.use_regex = Some(true);

        let result = handle_command(input.input).unwrap();
        assert_success_contains(&result, "Replaced 1 occurrences");
        verify_file_content(path, "Example");
    }

    #[test]
    fn test_str_replace_with_regex_multi() {
        let file = create_test_file("Test123 Test456 Test789");
        let path = file.path();

        let mut input = create_test_input("str_replace", path.to_str().unwrap());
        input.input.old_str = Some(r"Test\d+".to_string());
        input.input.new_str = Some("Example".to_string());
        input.input.use_regex = Some(true);
        input.input.allow_multi = Some(true);

        let result = handle_command(input.input).unwrap();
        assert_success_contains(&result, "Replaced 3 occurrences");
        verify_file_content(path, "Example Example Example");
    }

    #[test]
    fn test_str_replace_invalid_regex() {
        let file = create_test_file("Test content");

        let mut input = create_test_input("str_replace", file.path().to_str().unwrap());
        input.input.old_str = Some(r"Test[".to_string()); // Invalid regex (unclosed character class)
        input.input.new_str = Some("Example".to_string());
        input.input.use_regex = Some(true);

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRegex(_))));
    }
}

mod insert_tests {
    use super::*;

    #[test]
    fn test_insert_middle() {
        let file = create_test_file("Line 1\nLine 2\nLine 3");
        let path = file.path();

        let mut input = create_test_input("insert", path.to_str().unwrap());
        input.input.insert_line = Some(2);
        input.input.new_str = Some("New Line".to_string());

        let result = handle_command(input.input).unwrap();
        verify_edit_operation(&result, path, "Line 1\nLine 2\nNew Line\nLine 3");
    }

    #[test]
    fn test_insert_beginning() {
        let file = create_test_file("Line 1\nLine 2");
        let path = file.path();

        let mut input = create_test_input("insert", path.to_str().unwrap());
        input.input.insert_line = Some(0);
        input.input.new_str = Some("New First Line".to_string());

        let result = handle_command(input.input).unwrap();
        verify_edit_operation(&result, path, "New First Line\nLine 1\nLine 2");
    }

    #[test]
    fn test_insert_end() {
        let file = create_test_file("Line 1\nLine 2");
        let path = file.path();

        let mut input = create_test_input("insert", path.to_str().unwrap());
        input.input.insert_line = Some(2);
        input.input.new_str = Some("New Last Line".to_string());

        let result = handle_command(input.input).unwrap();
        verify_edit_operation(&result, path, "Line 1\nLine 2\nNew Last Line");
    }

    #[test]
    fn test_insert_invalid_line() {
        let file = create_test_file("Line 1\nLine 2");

        let mut input = create_test_input("insert", file.path().to_str().unwrap());
        input.input.insert_line = Some(5);
        input.input.new_str = Some("Invalid Line".to_string());

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_))));
    }
}

mod undo_tests {
    use super::*;

    #[test]
    fn test_undo_not_implemented() {
        let file = create_test_file("Original content");
        let path = file.path().to_str().unwrap();

        // Try to use undo_edit
        let input = create_test_input("undo_edit", path);
        let result = handle_command(input.input);

        // Should get UndoNotImplemented error
        assert!(matches!(result, Err(EditorError::UndoNotImplemented)));
    }
}

mod validation_tests {
    use super::*;

    #[test]
    fn test_invalid_command() {
        // We don't need any file for this test anymore, just testing the Command::from_str directly

        // Test just the invalid command handling directly
        let cmd_result = Command::from_str("invalid_command");
        assert!(matches!(cmd_result, Err(EditorError::UnknownCommand(_))));

        if let Err(EditorError::UnknownCommand(cmd)) = cmd_result {
            assert_eq!(cmd, "invalid_command");
        }
    }

    #[test]
    fn test_path_validation() {
        // Test relative path
        assert!(matches!(
            validate_path(Path::new("relative/path.txt"), &Command::View),
            Err(EditorError::NotAbsolutePath(_))
        ));

        // Test non-existent path for view command
        assert!(matches!(
            validate_path(Path::new("/nonexistent/file.txt"), &Command::View),
            Err(EditorError::PathNotFound(_))
        ));

        // Test existing path for create command
        let file = NamedTempFile::new().unwrap();
        assert!(matches!(
            validate_path(file.path(), &Command::Create),
            Err(EditorError::FileAlreadyExists(_))
        ));

        // Test using command other than view on directory
        let dir = tempdir().unwrap();
        assert!(matches!(
            validate_path(dir.path(), &Command::StrReplace),
            Err(EditorError::InvalidRange(_))
        ));
    }
}

mod delete_tests {
    use super::*;

    #[test]
    fn test_delete_lines() {
        let file = create_test_file("Line 1\nLine 2\nLine 3\nLine 4\nLine 5");
        let path = file.path();

        let mut input = create_test_input("delete", path.to_str().unwrap());
        input.input.delete_range = Some(vec![2, 4]);

        let result = handle_command(input.input).unwrap();
        assert_success_contains(&result, "Deleted lines 2-4");
        verify_file_content(path, "Line 1\nLine 5");
    }

    #[test]
    fn test_delete_missing_range() {
        let file = create_test_file("Test content");

        let input = create_test_input("delete", file.path().to_str().unwrap());

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::MissingDeleteRange)));
    }

    #[test]
    fn test_delete_invalid_range() {
        let file = create_test_file("Line 1\nLine 2\nLine 3");

        // End before start
        let mut input = create_test_input("delete", file.path().to_str().unwrap());
        input.input.delete_range = Some(vec![3, 1]);

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::InvalidRange(_))));

        // Out of bounds
        let mut input2 = create_test_input("delete", file.path().to_str().unwrap());
        input2.input.delete_range = Some(vec![5, 6]); // Well beyond file length

        let result2 = handle_command(input2.input);
        assert!(matches!(result2, Err(EditorError::InvalidRange(_))));
    }
}

mod create_tests {
    use super::*;

    #[test]
    fn test_create_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("new_file.txt");

        let mut input = create_test_input("create", file_path.to_str().unwrap());
        input.input.file_text = Some("This is new content".to_string());

        let result = handle_command(input.input).unwrap();
        assert_success_contains(&result, "File created successfully");
        verify_file_content(&file_path, "This is new content");
    }

    #[test]
    fn test_create_file_with_parent_dirs() {
        let dir = tempdir().unwrap();
        // Create path with non-existent parent directories
        let file_path = dir.path().join("nested/dirs/that/dont/exist/new_file.txt");

        let mut input = create_test_input("create", file_path.to_str().unwrap());
        input.input.file_text = Some("Content in nested directories".to_string());

        let result = handle_command(input.input).unwrap();
        assert_success_contains(&result, "File created successfully");
        verify_file_content(&file_path, "Content in nested directories");

        // Verify parent directories were created
        assert!(file_path.parent().unwrap().exists());
    }

    #[test]
    fn test_create_missing_file_text() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("new_file.txt");

        let input = create_test_input("create", file_path.to_str().unwrap());

        let result = handle_command(input.input);
        assert!(matches!(result, Err(EditorError::MissingFileText)));
    }
}
