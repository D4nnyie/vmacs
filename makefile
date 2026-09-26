BINARY := vmacs
TARGET := target/release/$(BINARY)
PREFIX ?= /usr/local

.PHONY: all build install uninstall clean

all: build

build:
	cargo build --release

install: build
	sudo install -Dm755 $(TARGET) $(DESTDIR)$(PREFIX)/bin/$(BINARY)

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BINARY)

clean:
	cargo clean