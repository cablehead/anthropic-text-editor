# Anthropic Text Editor

A Rust implementation of Anthropic's text editor tool for Claude, based on the
[Python implementation](https://github.com/anthropics/anthropic-quickstarts/blob/main/computer-use-demo/computer_use_demo/tools/edit.py).

This implements the `text_editor_20250124` tool described in the
[Anthropic documentation](https://docs.anthropic.com/en/docs/agents-and-tools/computer-use#understand-anthropic-defined-tools).

## Overview

This CLI tool provides file system operations for Claude to view, create, and
edit files. It communicates through a JSON protocol over stdin/stdout.

## Install

```bash
cargo install --locked anthropic-text-editor
```

## Supported Commands

- **view**: View file contents with line numbers (like `cat -n`) or directory listings
- **create**: Create a new file with provided content
- **str_replace**: Replace a specific string in a file (supports multiple replacements and regex patterns)
- **insert**: Insert text at a specific line in a file
- **delete**: Delete a range of lines from a file

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
    "command": "view|create|str_replace|insert|delete",
    "path": "/absolute/path/to/file",
    "view_range": [1, 10], // Optional, for view command on files
    "max_depth": 3, // Optional, for view command on directories (defaults to 3)
    "old_str": "text to replace", // Required for str_replace
    "new_str": "replacement text", // Optional for str_replace, required for insert
    "allow_multi": true, // Optional, for str_replace to allow multiple replacements
    "use_regex": true, // Optional, for str_replace to use regex pattern matching
    "delete_range": [1, 5], // Required for delete, specifies line range to remove
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

For the `view` command on files, the output includes line numbers:

```
"content": "Here's the result of running `cat -n` on /path/to/file.txt:\n     1\tLine 1\n     2\tLine 2\n     3\tLine 3\n"
```

## Usage

```
cat input.json | anthropic-text-editor
```

### Quick Examples

```bash
# View a file
echo '{"input":{"command":"view","path":"/path/to/file.txt"}}' | anthropic-text-editor

# Create a new file
echo '{"input":{"command":"create","path":"/path/to/new.txt","file_text":"Hello world"}}' | anthropic-text-editor

# Replace text in a file
echo '{"input":{"command":"str_replace","path":"/path/to/file.txt","old_str":"foo","new_str":"bar"}}' | anthropic-text-editor
```

### Example: Adding Content to README

Here's a meta example that shows how to use the tool to modify this README:

```json
{
  "input": {
    "command": "str_replace",
    "path": "/path/to/README.md",
    "old_str": "## Usage\n\n```\ncat input.json | anthropic-text-editor\n```",
    "new_str": "## Usage\n\n```\ncat input.json | anthropic-text-editor\n```\n\n### Quick Examples\n\n```bash\n# View a file\necho '{\"input\":{\"command\":\"view\",\"path\":\"/path/to/file.txt\"}}' | anthropic-text-editor\n\n# Create a new file\necho '{\"input\":{\"command\":\"create\",\"path\":\"/path/to/new.txt\",\"file_text\":\"Hello world\"}}' | anthropic-text-editor\n\n# Replace text in a file\necho '{\"input\":{\"command\":\"str_replace\",\"path\":\"/path/to/file.txt\",\"old_str\":\"foo\",\"new_str\":\"bar\"}}' | anthropic-text-editor\n```"
  }
}
```
