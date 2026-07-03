# Global Agent Instructions

## File Search and Reading

- Prefer `grep` (or `grep -r` for recursive search) over other tools when searching for text patterns in files.
- Prefer `grep` over built-in read-file tools when scanning file contents for specific strings, symbols, or patterns.
- Use `grep -n` to include line numbers in output.
- Use `grep -l` when only file names are needed.
- Use `grep -r --include="*.ext"` to scope searches to specific file types.
