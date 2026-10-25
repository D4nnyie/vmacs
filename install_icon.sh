
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ICON="$SCRIPT_DIR/vmacs.svg"
ICON_ROOT="/usr/share/icons"

if [[ ! -f "$ICON" ]]; then
    echo "Error: vmacs.svg not found next to the script."
    exit 1
fi

mapfile -d '' DIRS < <(
    find "$ICON_ROOT" -type d -path '*/16x16/apps' -print0
)

if [[ ${#DIRS[@]} -eq 0 ]]; then
    echo "No 16x16/apps directories found."
    exit 1
fi

sudo -v

for dir in "${DIRS[@]}"; do
    sudo install -m 644 "$ICON" "$dir/vmacs.svg"
    echo "Installed: $dir/vmacs.svg"
done

echo "Done! Installed vmacs.png into ${#DIRS[@]} directories."
