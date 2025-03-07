# Anthropic Text Editor

A Rust implementation of Anthropic's text editor tool for Claude, based on the
[Python implementation](https://github.com/anthropics/anthropic-quickstarts/blob/main/computer-use-demo/computer_use_demo/tools/edit.py).

This implements the `text_editor_20250124` tool described in the
[Anthropic documentation](https://docs.anthropic.com/en/docs/agents-and-tools/computer-use#understand-anthropic-defined-tools).

## Overview

This CLI tool provides file system operations for Claude to view, create, and
edit files. It communicates through a JSON protocol over stdin/stdout.

## Supported Commands

- **view**: View file contents or directory listings
- **create**: Create a new file with provided content
- **str_replace**: Replace a specific string in a file
- **insert**: Insert text at a specific line in a file

## Unsupported Commands

- **undo_edit**: Unlike the Python implementation, this command is not supported
  in the Rust version.

  The CLI is designed to be run by a wrapper that handles versioning through
  git. When Claude makes edits to files, the wrapper should commit those changes
  with git, allowing for easy version control and the ability to undo changes
  through git rather than through the editor itself. This simplifies the tool
  implementation while providing more robust version control.

## JSON Protocol

The CLI expects input in JSON format on stdin and produces JSON output on
stdout:

### Input Format

```json
{
  "input": {
    "command": "view|create|str_replace|insert",
    "path": "/absolute/path/to/file",
    "view_range": [1, 10], // Optional, for view command
    "old_str": "text to replace", // Required for str_replace
    "new_str": "replacement text", // Optional for str_replace, required for insert
    "insert_line": 5, // Required for insert
    "file_text": "content" // Required for create
  }
}
```

### Output Format

```json
{
  "content": "result of the operation",
  "is_error": true // Present only on error
}
```

## Usage

```
cat input.json | anthropic-text-editor
```
