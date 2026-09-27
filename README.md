# vmacs

**vmacs** is a lightweight, terminal-based text editor written in **Rust**.

The project aims to provide a fast, keyboard-driven editing experience while keeping the codebase simple, understandable, and easy to extend.

The name **vmacs** reflects the editor's current goal of combining ideas from both **Vim** and **Emacs** into a modern terminal editor built from the ground up in Rust.

> **Status:** Early development — functionality and keybindings are still evolving.

---

## Features & Functionality

### Editing

* [x] Text insertion
* [x] Text deletion
* [x] Newline insertion
* [x] Backspace / delete handling
* [ ] Tab support
* [x] Cursor movement
* [ ] Selection
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
* [ ] Command mode
* [ ] Visual mode
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

### Interface

* [x] Status bar
* [x] Command line
* [ ] Line numbers
* [~] Syntax highlighting
* [ ] Scrollbar / scroll indicators
* [ ] Error messages
* [~] Help screen

### Planned / Experimental

* [ ] Multiple windows / panes
* [ ] Plugins
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
* h/l/k/j - moves cursor
* dd  - deletes line
* yy  - yanks line
* x   - deletes one character
* p/P - paste / paste before
* i/I - enter insert mode / enter insert mode at the SOF
* a/A - enter insert mode after / enter insert mode at the EOF
* o/O - enter insert on new line below / enter insert on new line above
* $   - end of line
* w/b - word forwards/backwards
* D   - delete to line end
* more planned

**Commands**
 **Press : while in normal mode to start typing commands**
* :w - write file
* :q - quit
* :wq - write and quit
* :q! - force quit
* :w *filename* - saves file with specified name
* :wq *filename* - same as :w *filename*
* more planned

## Emacs

**Keybinds**
* C^a - start of line
* C^e - end of line
* C^f - move cursor right
* C^b - move cursor left
* C^n - move cursor down
* C^p - move cursor up
* C^d - delete
* C^k - delete to line end
* C^y - paste in line
* C^x - save to file
* C^s - search
* C^w - save as
* M^f - word forward
* M^b - word backwards
* more planned

## Both

* The arrow keys work for all modes.
* The copy/cut/paste keybinds are all linked, C^y will work with yy and p will work with M^w when added.
* Emacs keybinds are universal in all modes. Vim works only in normal mode.
* Ctrl + Shift + V currently pastes anything copied from other applications. It does not sync with the editor's yank. Neither does Ctrl + Shift + C overwrite it.
  

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
