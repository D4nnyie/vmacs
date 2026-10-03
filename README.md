# vmacs

**vmacs** is a lightweight, terminal-based text editor written in **Rust**.

The project aims to provide a fast, keyboard-driven editing experience while keeping the codebase simple, understandable, and easy to extend.

The name **vmacs** reflects the editor's current goal of combining ideas from both **Vim** and **Emacs** into a modern terminal editor built from the ground up in Rust.

> **Status:** Early development — functionality and keybindings are still evolving.

---

## Features & Functionality

* [~] - Incomplete | limited functionality added.

### Editing

* [x] Text insertion
* [x] Text deletion
* [x] Newline insertion
* [x] Backspace / delete handling
* [ ] Tab support
* [x] Cursor movement
* [x] Selection
* [~] Copy / cut / paste
* [ ] Undo / redo
* [x] Multiple lines
* [ ] Multiple buffers

### Navigation

* [x] Character navigation
* [x] Word navigation
* [x] Line navigation
* [x] Beginning / end of line
* [x] Beginning / end of buffer
* [ ] Page navigation
* [ ] Jump to line
* [x] Mouse support

### File Management

* [x] Open files
* [x] Create files
* [x] Save files
* [x] Save As
* [ ] File path handling
* [x] Unsaved-change detection

### Editor Modes

* [x] Insert mode
* [x] Normal mode
* [x] Visual mode
* [x] Mode indicators

### Commands

* [~] Command prompt
* [ ] File commands
* [ ] Search commands
* [ ] Editor configuration
* [ ] Custom commands

### Search & Editing Tools

* [x] Text search
* [~] Search navigation
* [ ] Find and replace
* [ ] Case-sensitive search
* [ ] Regular expressions

### Configuration

* [ ] User configuration file
* [ ] Custom keybindings
* [ ] Editor preferences
* [ ] Theme configuration
* [ ] Customizable settings
* [x] Custom language support

### Interface

* [x] Status bar
* [x] Command line
* [~] Line numbers
* [~] Syntax highlighting
* [x] Scrollbar / scroll indicators
* [ ] Error messages
* [~] Help screen

### Planned / Experimental

* [ ] Multiple windows / panes
* [~] Plugins
* [ ] Macros
* [ ] LSP support
* [ ] Git integration
* [ ] Project navigation
* [ ] Terminal integration

---

## Why vmacs?

There are already many excellent terminal editors. vmacs isn't intended to replace them.

The project exists primarily as an exploration of:

* Learning Rust through a practical project
* Designing a modal/keyboard-driven interface
* Combining editing concepts from different editors
* Creating a codebase that is small enough to understand and modify

The goal is to keep vmacs **fast, minimal, and extensible** without sacrificing useful functionality.

---

## Keybindings

vmacs is designed around keyboard-driven interaction.

The keybindings are currently under development and may change as the editor evolves.

### Current Philosophy

The editor takes inspiration from both Vim and Emacs rather than attempting to strictly follow either editor.

As the project develops, keybindings will be documented here.

## Vim

**Keybinds**
* h - Left
* l - Right
* k - Up
* j - Down
* 0 - SOL (Start of line)
* $ - EOL (End of line)
* w - word forwards
* b - word Backspace
* x - delete char
* D - delete to line end
* p - Paste
* P - paste before
* i - input mode
* a - input mode, after cursor
* o - input mode, new line below
* I - input mode, line start
* A - input mode, line end
* O - input mode, new line above
* dd - delete line
* yy - yank line
* gg - start of buffer
* G - end of  buffer
* : - command prompt
* v - mark mode
* more planned

**Commands**
* :q - quit
* :q! - force quit
* :w - write
* :wq/:x - write and quit
* :w <filename> - can save file with specified filename and format
* :w /path/to/file - can save file into a specific path with a specific name and format
* more planned

## Emacs

**Keybinds**
* C^a - SOL
* C^e - EOL
* C^f - Right
* C^b - Left
* C^n - Down
* C^p - Up
* C^d - Delete
* C^k - Delete to line end
* C^y - Paste in line
* C^h - Delete Backwards (Backspace alternative)
* M^f - word forwards
* M^b - word Backwards
* M^w - save as
* C^x - save
* C^s - search
* C^q - quit
* M^< - start of buffer
* M^> - end of buffer
* more planned

## Mark Mode (Visual)
* h - Left
* l - Right
* k - Up
* j - Down
* 0 - SOL
* $ - EOL
* w - word forwards
* b - word Backspace
* y - yank selected
* d/x - delete selected
* u - lowercase selected
* U - uppercase selected
* \> - indent selected
* < - dedent selected
* g - start of buffer
* G - end of buffer
* v/ESC - exit visual mode
* Ctrl + h & Backspace - can delete without being forced back to normal mode.

## Universal

* The arrow keys work for all modes.
* The copy/cut/paste keybinds are all linked, C^y will work with yy and p will work with M^w when added.
* Emacs-alike keybinds are universal in all modes.
* Ctrl + Shift + V pastes anything copied from the system's clipboard. It does not sync with the editor's yank.
* Ctrl + Shift + Y copies to system's clipboard.
* C^Home - start of buffer.
* C^End  - end of buffer.
* Mouse dragging should work in all available modes.
* C^h can be used as an alternative if Backspace problems occur.

---

## Platform Support

vmacs is actively developed on linux, so support for other platforms is not guaranteed.

### Supported / Intended Platforms:
* [x] Linux ```Tested```
* [ ] macOS ```Untested```
* [ ] Windows ```Untested```
* [ ] BSD and other unix-like platforms ```Untested```

vmacs is designed to be cross-platform and runs in a terminal, using Rust and Crossterm for terminal interaction.

If you encounter a platform-specific issue, please open an issue with your operating system, terminal emulator, and relevant error/output.

---

## Installation

### From Source

Make sure you have a working Rust installation with Cargo.

Clone the repository:

```bash
git clone https://github.com/D4nnyie/vmacs.git
cd vmacs
```

Build the project:

```bash
cargo build --release
```

Run it with:

```bash
cargo run --release
```

The compiled binary can be found in:

```text
target/release/vmacs
```

You can optionally install it somewhere in your `PATH`:

```bash
cp target/release/vmacs ~/.local/bin/
```

> Installation instructions may change as the project develops.

### Makefile

Clone the repository:

```bash
git clone https://github.com/D4nnyie/vmacs.git
cd vmacs
```

Run:
```bash
make install
```

---

## Configuration

vmacs is planned to support a user configuration file located at:

```text
~/.vmacsrc
```

Configuration functionality is currently under development.

Future configuration options may include:

* Keybindings
* Editor behavior
* Appearance
* Tabs and indentation
* Search preferences
* Custom commands

---

## Custom language

vmacs supports simple custom languages syntax. These can be added at ~/.config/vmacs/languages, %APPDATA%\vmacs\languages\ or $XDG_CONFIG_HOME/vmacs/languages/. An example toml file can be found there.

Detailed explanation:

```toml
# Custom language configuration for vmacs.
#
# Save this file as:
#   ~/.config/vmacs/languages/mylang.toml
#
# You can also use the platform's standard config directory:
#   Linux/macOS/BSD: $XDG_CONFIG_HOME/vmacs/languages/
#   Windows:         %APPDATA%\vmacs\languages\

# Display name of the language.
name = "MyLang"

# File extensions that should be recognized as this language.
# For example, "file.myl" and "file.ml2" will use this configuration.
extensions = ["myl", "ml2"]

# Words that should be highlighted as language keywords.
keywords = ["task", "emit", "when", "else", "end", "let"]

# Built-in type names used by the language.
types = ["Int", "Text", "Bool"]

# Built-in constants or special values.
known_values = ["yes", "no", "nil"]

# Single-line comment delimiter.
# Everything after this delimiter is treated as a comment until the end of the line.
line_comment = "#"

# Multi-line comment delimiters.
# The first value opens the comment and the second closes it.
# Example: /* comment */
block_comment = ["/*", "*/"]

# String delimiters.
# Each pair contains an opening and closing delimiter.
# The longest matching opener is preferred when multiple delimiters could match.
#
# This example supports:
#   "normal string"
#   'single-quoted string'
#   """multi-line string"""
string_delimiters = [["\"", "\""], ["'", "'"], ["\"\"\"", "\"\"\""]]

# String delimiters that are allowed to continue across multiple lines.
# These must also be present in string_delimiters above.
multiline_strings = ["\"\"\""]

# Characters or keywords that trigger automatic indentation
# when pressing Enter after them.
#
# For example:
#   task {
#       <- automatically indented
#
# You can use symbols such as "{", "(", "[" or language keywords
# such as "do" and "then".
indent_triggers = ["{", "(", "[", "do", "then"]

# Whether keywords, types, and other language elements are
# matched without considering uppercase/lowercase differences.
#
# false:
#   "Task" and "task" are different
#
# true:
#   "Task" and "task" are treated as the same
case_insensitive = false

# Whether the language has a separate character literal syntax.
#
# Set this to true for languages that support things such as:
#   'a'
#
# Set this to false if single quotes are only used for strings.
supports_char_literal = false

# Whether the language uses lifetime syntax, such as Rust's:
#   'a
#
# This is mainly useful for languages with Rust-style lifetimes.
supports_lifetime = false
```

---

## Building

vmacs uses Cargo for building and dependency management.

Debug build:

```bash
cargo build
```

Release build:

```bash
cargo build --release
```

Run directly with Cargo:

```bash
cargo run
```

---

## Project Structure

The project is organized into separate components to keep the editor's functionality modular.

The structure will evolve as more editor functionality is implemented.

---

## Dependencies

vmacs is written in Rust and uses crates from the Rust ecosystem where appropriate.

Current dependencies can be found in:

```text
Cargo.toml
```

---

## Contributing

Contributions, suggestions, bug reports, and ideas are welcome.

If you want to contribute:

1. Fork the repository.
2. Create a new branch.
3. Make your changes.
4. Test your changes.
5. Open a pull request.

---

## Acknowledgements

vmacs was initially developed while following **Philipp Flenker's "Build Your Own Text Editor in Rust" tutorial**.

The tutorial was an important learning resource for understanding how a terminal text editor can be implemented in Rust and served as an early source of inspiration for vmacs.

Even though some parts of the code are similar with the original editor, the goal is to change it until it feels like a new editor rather than a copy.

* **Tutorial:** [Build Your Own Text Editor in Rust](https://github.com/pflenker/hecto-tutorial)
* **Author:** Philipp Flenker
* **Example project:** [hecto](https://github.com/pflenker/hecto)

---

## License

vmacs is free and open-source software licensed under the **GNU General Public License v3.0**.

See the [`LICENSE`](LICENSE) file for the complete license text.

GPL-3.0 allows users to use, study, modify, and redistribute the software, while requiring distributed modified versions covered by the license to preserve the corresponding freedoms.

---

<p align="center">
  <sub>Built with Rust 🦀</sub>
</p>
