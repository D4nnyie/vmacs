# `<\:>` vMACS — Versatile Modal And Command System

**vMACS** is a lightweight, terminal-based text editor written in **Rust**, designed to combine ideas from Vim, Emacs, and other keyboard-driven editors into a single, extensible editing environment.

Built around a modal editing system, vMACS aims to provide efficient keyboard-driven workflows, practical editing tools, customizable behavior, and a codebase that remains understandable and easy to extend.

Rather than strictly replicating an existing editor, vMACS takes inspiration from different approaches to text editing to develop its own identity.

> **Development status:** Late prototype / early-stage development. vMACS is functional and actively evolving. Features, keybindings, and configuration options may change between releases.

---

## Features

### Editing

- [x] Text insertion and deletion
- [x] Newline insertion and deletion handling
- [x] Tab support
- [x] Cursor movement
- [x] Text selection
- [x] Copy, cut, and paste
- [x] Undo and redo
- [x] Multi-line editing
- [ ] Multiple buffers

### Navigation

- [x] Character and word navigation
- [x] Line navigation
- [x] Beginning and end of line
- [x] Beginning and end of buffer
- [x] Page navigation
- [x] Jump to line
- [x] Mouse navigation and interaction

### File Management

- [x] Open existing files
- [x] Create files that do not exist
- [x] Save and Save As
- [x] File path handling
- [x] Unsaved-change detection
- [x] Built-in file manager
- [x] Recent files dashboard

### Editing Modes

vMACS uses three editing modes:

- [x] **CONTROL** — Modal navigation and command-oriented editing, inspired by Vim's normal mode.
- [x] **INPUT** — Text insertion and modification, similar to Vim's insert mode.
- [x] **MARK** — Text selection and visual editing, inspired by Vim's visual mode.
- [x] Mode indicators

### Commands

- [x] Command prompt
- [~] File commands — implemented, with functionality still evolving
- [x] Search commands
- [x] Search and replace
- [x] Go-to-line commands
- [x] Configuration commands
- [x] Custom command prompt (`\`)
- [x] Detailed error messages

### Search and Editing Tools

- [x] Text search
- [x] Search result navigation
- [x] Find and replace
- [x] Case-sensitive search
- [x] Regular-expression search
- [x] System clipboard integration
- [x] Context-dependent middle-click copy and paste

### Configuration and Extensibility

- [x] User configuration file
- [x] Configurable mouse behavior
- [x] Custom language configurations
- [x] User-defined commands
- [ ] Custom keybindings
- [ ] Theme configuration
- [ ] Comprehensive editor preferences

### Interface

- [x] Dashboard
- [x] Redesigned editor interface
- [x] Status bar and command line
- [x] Line numbers
- [x] Current-line highlighting
- [x] Syntax highlighting
- [x] Scroll indicators
- [x] Selection highlighting, including empty lines
- [x] Help screen
- [x] Error reporting

### Planned and Experimental

- [ ] Multiple windows or panes
- [~] Extended plugin functionality
- [ ] Macros
- [ ] Language Server Protocol (LSP) support
- [ ] Git integration
- [ ] Project navigation
- [ ] Integrated terminal

**Legend:** `[x]` Implemented · `[~]` Partially implemented or still evolving · `[ ]` Planned

---

## Why vMACS?

There are already many excellent terminal editors, and vMACS is not intended to replace them.

The project is an exploration of several ideas:

- **Learning Rust** through the development of a practical application.
- **Modal editing** that makes navigation and editing efficient without relying entirely on the mouse.
- **Combining editing philosophies** from Vim, Emacs, and other editors.
- **Extensibility** through custom language definitions and user-defined commands.
- **Simplicity and maintainability** without sacrificing useful functionality.
- **Independent design** through a distinct interface, color scheme, and command system.

The long-term goal is to make vMACS **fast, practical, configurable, and extensible**, while keeping its implementation understandable.

---

## Keybindings and Editing Modes

vMACS is designed around keyboard-driven interaction, with three modes: CONTROL, INPUT, and MARK.

Its keybindings take inspiration from Vim and Emacs without attempting to reproduce either editor exactly. Keybindings and workflows may evolve as development continues.

To view the current keybindings:

- Press `h` on the dashboard.
- Press `M-h` in any mode.
- Run `:help` in the editor's command prompt.

Refer to the built-in help screen for the keybindings supported by your installed version.

---

## Platform Support

vMACS is primarily developed and tested on Linux. Other platforms are intended targets but have not been fully verified.

| Platform | Status |
|---|---|
| Linux | Tested |
| macOS | Untested |
| Windows | Untested |
| BSD and other Unix-like systems | Untested |

vMACS uses Rust and Crossterm for terminal interaction, providing a foundation for cross-platform support. However, compatibility may depend on the operating system, terminal emulator, toolchain, and system libraries.

If you encounter a platform-specific issue, please open a GitHub issue and include your operating system, terminal emulator, Rust version, and relevant error output.

---

## Requirements

- **Rust 1.90 or newer** to build vMACS.
- A working Rust installation with Cargo.
- A **Powerline-compatible font** for the intended interface appearance.
- A compatible terminal emulator.

The editor is developed primarily on Linux. Compatibility with other platforms is not guaranteed at this stage.

---

## Installation

### Build from source

Clone the repository:

```bash
git clone https://github.com/D4nnyie/vmacs.git
cd vmacs
```

Build the optimized release:

```bash
cargo build --release
```

Run vMACS:

```bash
cargo run --release
```

The compiled executable is located at:

```text
target/release/vmacs
```

### Install manually

You can copy the executable into a directory in your `PATH`:

```bash
mkdir -p ~/.local/bin
cp target/release/vmacs ~/.local/bin/
```

Make sure `~/.local/bin` is included in your `PATH`.

### Install using Make

From the repository directory, run:

```bash
make install
```

The Makefile handles installation according to its configured installation prefix and may require elevated privileges, depending on the destination.

> Installation instructions may evolve as vMACS develops. Check the repository for the latest build and installation requirements.

---

## Configuration

vMACS uses a TOML configuration file located at:

```text
~/.config/vmacs/config.toml
```

The configuration file controls supported editor settings, including configurable mouse behavior and custom commands.

If the configuration file has not been created automatically, run:

```text
:config
:reload
```

Use `:reload` after changing configuration when you need vMACS to reload its settings.

The exact options supported depend on the current version. Refer to the example configuration and comments provided by the project.

### Custom commands

vMACS provides a separate custom-command prompt, opened using the `\` command prefix.

Custom commands are defined in `config.toml`, allowing users to configure their own commands without modifying the editor's source code.

Custom commands have resource limits to keep configuration loading and execution manageable. See the configuration examples and implementation for the current limits.

---

## Custom Language Support

vMACS supports user-defined language configurations through TOML files. These configurations let you define basic syntax-highlighting rules and selected language behavior without implementing a dedicated language module.

### Configuration directories

Place custom language configurations in the appropriate directory for your system:

| Platform | Directory |
|---|---|
| Linux | `~/.config/vmacs/languages/` |
| Linux or Unix-like systems using XDG | `$XDG_CONFIG_HOME/vmacs/languages/` |
| macOS | `~/.config/vmacs/languages/` or the configured XDG directory |
| Windows | `%APPDATA%\vmacs\languages\` |

If `XDG_CONFIG_HOME` is set, use that directory instead of the default `~/.config` location.

Each language configuration should be saved as a `.toml` file. An example configuration is provided below.

### Example language configuration

The following example defines a fictional language called `MyLang`.

Save it as `mylang.toml` in your language configuration directory.

```toml
# Display name of the language.
name = "MyLang"

# File extensions recognized by this configuration.
extensions = ["myl", "ml2"]

# Keywords, built-in types, and known values.
keywords = ["task", "emit", "when", "else", "end", "let"]
types = ["Int", "Text", "Bool"]
known_values = ["yes", "no", "nil"]

# Single-line comments.
line_comment = "#"

# Multi-line comments: opening delimiter, closing delimiter.
block_comment = ["/*", "*/"]

# String delimiter pairs.
string_delimiters = [
    ['"', '"'],
    ["'", "'"],
    ['"""', '"""']
]

# Delimiters that can span multiple lines.
# These must also be declared in string_delimiters.
multiline_strings = ['"""']

# Tokens that trigger automatic indentation after Enter.
indent_triggers = ["{", "(", "[", "do", "then"]

# Whether matching ignores letter case.
case_insensitive = false

# Whether single-quoted character literals are supported.
supports_char_literal = false

# Whether Rust-style lifetime syntax is supported.
supports_lifetime = false
```

### Available language options

| Option | Purpose |
|---|---|
| `name` | Display name of the language |
| `extensions` | File extensions associated with the language |
| `keywords` | Words highlighted as keywords |
| `types` | Built-in type names |
| `known_values` | Built-in constants and special values |
| `line_comment` | Single-line comment delimiter |
| `block_comment` | Opening and closing delimiters for block comments |
| `string_delimiters` | Supported string opening and closing delimiters |
| `multiline_strings` | String delimiters that may span multiple lines |
| `indent_triggers` | Tokens that trigger automatic indentation |
| `case_insensitive` | Whether language-element matching ignores case |
| `supports_char_literal` | Whether character-literal syntax is supported |
| `supports_lifetime` | Whether Rust-style lifetime syntax is supported |

Custom language configurations provide basic syntax support; they are not a substitute for a full parser or language server.

### Configuration limits

To keep resource usage predictable, vMACS imposes limits on user-defined configurations. Current limits include:

- Language and main configuration file sizes: **128 KiB**.
- Custom language configuration files: **32**.
- Keywords, types, and known values: **2,000 entries each**, with a maximum of **64 bytes per word**.
- String delimiter pairs and multiline string delimiters: **16 each**.
- Delimiters, comment markers, and indentation triggers: **1–16 bytes**, without control characters.
- Extensions and filenames: **32 entries each**, with a maximum of **64 bytes per entry**.
- Custom commands: **256 commands**, with up to **32 lines per command** and **1,024 bytes per line**.

These limits may change in future releases. Increasing them directly in the source code is possible in some cases, but larger limits may increase memory usage and should be changed only when necessary.

---

## Building and Development

vMACS uses Cargo for building and dependency management.

Debug build:

```bash
cargo build
```

Optimized release build:

```bash
cargo build --release
```

Run in debug mode:

```bash
cargo run
```

Run the optimized release:

```bash
cargo run --release
```

The project's dependencies and their versions are declared in `Cargo.toml`.

---

## Project Structure

vMACS is organized into separate Rust modules to keep its editing, interface, configuration, and language-related functionality modular.

The internal structure may change as development continues. Contributions should follow the existing project organization and avoid unnecessary coupling between components.

---

## Contributing

Contributions, suggestions, bug reports, and feature requests are welcome.

If you want to contribute:

1. Fork the repository.
2. Create a branch for your changes.
3. Implement and test your changes.
4. Verify that existing functionality continues to work.
5. Open a pull request describing the changes.

Because vMACS is still evolving, please check existing issues and discussions before starting large changes. Compatibility and file safety are important considerations for any editing-related contribution.

---

## Acknowledgements

vMACS was initially developed while following **Philipp Flenker's "Build Your Own Text Editor in Rust" tutorial**.

The tutorial provided an important foundation for understanding how a terminal-based text editor can be implemented in Rust. It served as an early learning resource and source of inspiration while vMACS developed its own functionality and design.

- **Tutorial:** [Build Your Own Text Editor in Rust](https://github.com/pflenker/hecto-tutorial)
- **Author:** Philipp Flenker
- **Example project:** [hecto](https://github.com/pflenker/hecto)

---

## License

vMACS is free and open-source software licensed under the **GNU General Public License v3.0 (GPL-3.0)**.

See the [`LICENSE`](LICENSE) file for the complete license text.

The GPL-3.0 permits users to use, study, modify, and redistribute the software under its terms, including requirements that apply when distributing modified versions.

---

<p align="center">
  <sub>Built with Rust 🦀</sub>
</p>
