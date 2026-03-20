#!/usr/bin/env bash
# Interclaude dependency installer
# Detects OS and installs required dependencies

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

check() {
    if command -v "$1" &>/dev/null; then
        echo -e "  ${GREEN}[+]${NC} $1 $(command -v "$1")"
        return 0
    else
        echo -e "  ${RED}[-]${NC} $1 not found"
        return 1
    fi
}

echo "=== Interclaude Dependency Check ==="
echo ""

OS=$(uname -s)
echo "Platform: $OS"
echo ""

MISSING=()

echo "Required:"
check ssh || MISSING+=("ssh")
check rsync || MISSING+=("rsync")

echo ""
echo "Recommended:"
check mosh || MISSING+=("mosh")
check autossh || MISSING+=("autossh")

echo ""
echo "Optional:"
check fswatch || check inotifywait || MISSING+=("fswatch/inotifywait")
check redis-cli || echo -e "  ${YELLOW}[~]${NC} redis-cli (only needed for Redis transport)"
check claude || echo -e "  ${YELLOW}[~]${NC} claude CLI (install from https://claude.ai/download)"

echo ""

if [ ${#MISSING[@]} -eq 0 ]; then
    echo -e "${GREEN}All dependencies satisfied!${NC}"
    exit 0
fi

echo -e "${YELLOW}Missing: ${MISSING[*]}${NC}"
echo ""

if [ "$OS" = "Darwin" ]; then
    echo "Install on macOS:"
    echo "  brew install mosh autossh rsync fswatch redis"
elif [ "$OS" = "Linux" ]; then
    echo "Install on Debian/Ubuntu:"
    echo "  sudo apt install mosh autossh rsync inotify-tools redis-tools"
    echo ""
    echo "Install on RHEL/Fedora:"
    echo "  sudo dnf install mosh autossh rsync inotify-tools redis"
fi
