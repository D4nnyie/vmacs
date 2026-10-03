BINARY := vmacs

TARGET := target/release/$(BINARY)

PREFIX ?= /usr/local

CONFIG_DIR := $(HOME)/.config/vmacs
LANGUAGE_DIR := $(CONFIG_DIR)/languages
LANGUAGE_FILE := $(LANGUAGE_DIR)/mylang.toml

.PHONY: all build install uninstall clean

all: build

build:
	cargo build --release

install: build
	sudo install -Dm755 $(TARGET) $(DESTDIR)$(PREFIX)/bin/$(BINARY)
	mkdir -p $(LANGUAGE_DIR)
	@if [ ! -f "$(LANGUAGE_FILE)" ]; then \
		printf '%s\n' \
			'# Example custom language configuration' \
			'name = "MyLang"' \
			'extensions = ["myl", "ml2"]' \
			'' \
			'keywords = ["task", "emit", "when", "else", "end", "let"]' \
			'types = ["Int", "Text", "Bool"]' \
			'known_values = ["yes", "no", "nil"]' \
			'' \
			'line_comment = "#"' \
			'block_comment = ["/*", "*/"]' \
			'' \
			'# [open, close] pairs. Longest matching opener wins.' \
			'string_delimiters = [["\"", "\""], ["'\''", "'\''"], ["\"\"\"", "\"\"\""]]' \
			'' \
			'# Openers (from the list above) whose strings may span several lines.' \
			'multiline_strings = ["\"\"\""]' \
			'' \
			'# Pressing Enter after a line ending with one of these indents the next line.' \
			'indent_triggers = ["{", "(", "[", "do", "then"]' \
			'' \
			'case_insensitive = false' \
			'supports_char_literal = false' \
			'supports_lifetime = false' \
			> "$(LANGUAGE_FILE)"; \
		echo "Created $(LANGUAGE_FILE)"; \
	else \
		echo "$(LANGUAGE_FILE) already exists, leaving it untouched"; \
	fi

uninstall:
	sudo rm -f $(DESTDIR)$(PREFIX)/bin/$(BINARY)

clean:
	cargo clean
