# Anthropic Text Editor Reference

## Initial Exploration
- Run `git ls-files` to get an overview of project files

## Work Cycle Pattern:

1. Make changes to code
2. `cargo test` to verify functionality
3. `cargo fmt` to ensure consistent formatting
4. `cargo clippy` to catch common mistakes and inefficiencies
5. Fix any issues found in steps 2-4
6. Commit with conventional commit message:
   - Format: `<type>: <description>`
   - Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`
   - Example: `feat: add configurable directory traversal depth`