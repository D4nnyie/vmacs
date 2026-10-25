
#!/usr/bin/env bash
set -euo pipefail

APP_NAME="vMACS"
BINARY="vmacs"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_DIR="$HOME/.local/share/applications"
DESKTOP_FILE="$DESKTOP_DIR/$BINARY.desktop"

ICON_DIR="$HOME/.local/share/icons/hicolor/16x16/apps"
ICON_SOURCE="$SCRIPT_DIR/vmacs.svg"
ICON_FILE="$ICON_DIR/vmacs.svg"

case "${1:-install}" in
    install)
        mkdir -p "$DESKTOP_DIR" "$ICON_DIR"

        cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Name=$APP_NAME
Comment=Versatile Command And Control System
Exec=$BINARY
Icon=vmacs
Terminal=true
Type=Application
Categories=Utility;TextEditor;Development;
StartupNotify=false
EOF

        if [[ -f "$ICON_SOURCE" ]]; then
            install -m 644 "$ICON_SOURCE" "$ICON_FILE"
            echo "Installed icon: $ICON_FILE"
        else
            echo "Warning: vmacs.svg not found; skipping icon installation"
        fi

        echo "Installed desktop entry: $DESKTOP_FILE"
        ;;

    uninstall)
        rm -f "$DESKTOP_FILE" "$ICON_FILE"
        echo "Removed vMACS desktop entry and user icon"
        ;;

    *)
        echo "Usage: $0 {install|uninstall}"
        exit 1
        ;;
esac
