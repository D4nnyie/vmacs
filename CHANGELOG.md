# vMACS Changelog

All notable changes to vMACS are documented here, in reverse chronological order.

## V1.6.0 — The Workflow Update

A major update focused on improving the editing workflow, navigation, and overall usability of vMACS. (also a bit of redesign of V1.5.0 aswell)

### Added

#### Buffer Management
- Added support for multiple buffers.
- Added a top bar for displaying and switching between buffers.
- Added indicators for buffers with unsaved changes.
- Added dashboard pins for quick access to frequently used files.
- Redesigned the recent files interface and added filtering.
- Added configurable shortcuts for opening recent files:
  - `R` + `<number>`
  - `Shift` + `<number>`
- Added a toggleable full file path display in the status bar, accessible through `:path` or `Alt+P`.

#### Navigation
- Added a Vim-style jump list.
- Added screen-relative navigation using Vim's `H`, `M`, and `L` motions.
- Added viewport scrolling.
- Added additional Vim-style motions and commands.
- Added universal motion keybindings, documented in `:help`.
- Added cursor column memory to preserve the desired column during vertical navigation.
- Added horizontal scrolling. (Horizontal scroll wheel support)

#### Editing
- Added code folding, accessible through `:fold` or the fold button beside a line.
- Added selection replacement in Input mode: typing over selected text replaces it.
- Improved cursor behavior in Control and Mark modes to better match Vim.
- Added autocompletion. (from buffer)

#### Interface and Usability
- Introduced Workstation mode with three configurable dashboard layouts (Left, Center, Right).
- When scaling down the terminal, Workstation defaults to Center layout when too small.
- Added dashboard icons, with support for disabling them or using Nerd Font icons.
- Added compact UI mode, configurable through `:ui full`, `:ui compact`, `:ui ws`, or the configuration file.
- Added support for solid-color and transparent backgrounds.
- Redesigned the Mark mode cursor for easier selection.
- Changed the Home key behavior to SmartHome.
- Added file previews to the file manager.
- Redesigned the file manager's filter bar.
- Redesigned scope guides and enabled them by default alongside bracket highlighting.
- Added animations. (disabled in compact mode.)

#### Configuration and Customization
- Added automatic configuration file creation.
- Added custom keybindings for existing editor actions and custom commands.
- Added optional yank history with configurable entry limits and maximum entry sizes.

#### File Handling
- Added support for saving files with different encodings through `:encoding`.
- Added support for configuring end-of-line formats through `:eol`.

### Changed
- Expanded the available Vim-style commands and keybindings. See `:help` for details.
- Refined cursor behavior and navigation throughout the editor.

### Fixed
- Fixed various bugs and improved overall stability.
- Fixed interface rendering issues after terminal resizing in window managers. (Sometimes it takes a seccond)
- Fixed command usage information being clipped by command descriptions in the command bar.

### Notes
- Yank history is disabled by default.
- `max_entries` defaults to `20` and is capped at `100`.
- `max_entry_bytes` defaults to `4096` and is capped at `65536` bytes (64 KiB).
- Both yank history limits can be customized in the configuration file.
- A Powerline-capable font and a Nerd Font are recommended but not required. Unicode alternatives are available for icons, although mode indicators may look unusual without Powerline-compatible glyphs.

### Requirements
- Rust 1.90 or newer.

---

## V1.5.0 — The Overhaul Update / Identity Update

A major release focused on redesigning the editor interface, improving usability, and expanding built-in functionality.

### Added
- Added a dashboard.
- Added a file manager for easier file navigation and opening.
- Added a help screen.
- Added line numbers and current-line highlighting.
- Added search and replace.
- Added system clipboard integration in Control and Mark modes.
- Added a custom commands prompt, accessible through `\`.
- Added configurable mouse settings.
- Added a configurable command system through `~/.config/vmacs/config.toml`.
- Added a go-to-line function, accessible through `:goto`, `:go`, and `M-g`.
- Added detailed error messages.
- Added selection highlighting for empty lines.
- Added support for creating new files when the specified path does not exist.

### Changed
- Redesigned the editor interface.
- Finalized the default color scheme.
- Improved the syntax highlighting system.
- Expanded mouse functionality.
- Middle-click now copies selected text or pastes when no text is selected. When clipboard text is already available, middle-click can perform both operations as applicable.

### Fixed
- Fixed editor stalls when pasting long lines.
- Fixed pasted lines extending beyond the visible screen.
- Fixed major bugs that could jeopardize file contents.
- Fixed various additional bugs.

### Configuration and Limits
- Configuration and language file sizes are limited to 128 KiB.
- A maximum of 32 custom language files is supported.
- `keywords`, `types`, and `known_values` support up to 2,000 entries each, with a maximum of 64 bytes per entry.
- `string_delimiters` and `multiline_strings` support up to 16 entries each.
- Delimiters, comment markers, and indentation triggers must be between 1 and 16 bytes and contain no control characters.
- `extensions` and `filenames` support up to 32 entries each, with a maximum of 64 bytes per entry.
- Custom commands support up to 256 commands, with a maximum of 32 lines per command and 1,024 bytes per line.
- Some limits can be increased by modifying `editor/config.rs` and `registry/plugin.rs`. This is not recommended because larger limits may increase memory usage. Keep increases modest and test thoroughly before exceeding the defaults.

### Notes
- The configuration file is located at `~/.config/vmacs/config.toml`.
- If the configuration file is not created automatically, run `:config` followed by `:reload`.

### Requirements
- Rust 1.90 or newer.
- A Powerline-capable font is recommended for the interface.

---

## V1.4.0 — Syntax and Modes

### Added
- Added basic language syntax highlighting.
- Added support for custom language plugins.

### Changed
- Renamed the editing modes to reflect vMACS's Vim- and Emacs-inspired design.

### Notes
- V1.3.0 was skipped. Minor bug fixes and system clipboard integration were part of the development leading up to V1.4.0.

---

## V1.2.8 — Visual Editing

### Added
- Added Visual mode for selecting text.
- Added a scrollbar.
- Added additional editor functions and keybindings.

### Changed
- Reworked the editor renderer.

### Fixed
- Fixed backspace bugs that could make the feature unusable.

---

## V1.1.4 — Workflow Improvements

### Added
- Added Meta/Alt keybindings for forward and backward word movement: `M-f` and `M-b`.
- Added a Save As function through the `C-w` keybinding.
- Added a Makefile for building and installing the editor.

---

## V1.1.2 — Initial Release

- Initial release of vMACS.
