#!/usr/bin/env bash

set -e

# Formatting variables
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== searchxyz Local Update Script ===${NC}\n"

# 1. Pull latest code if in a Git repository
if [ -d .git ]; then
    echo -e "${BLUE}[1/3] Checking for remote updates in Git...${NC}"
    # Check if there's a remote tracking branch configured
    if git rev-parse --abbrev-ref --symbolic-full-name @{u} &>/dev/null; then
        echo -e "${YELLOW}Running: git pull${NC}"
        git pull || echo -e "${YELLOW}Warning: Git pull failed. Rebuilding current local directory state.${NC}"
    else
        echo -e "${YELLOW}No remote tracking branch configured. Rebuilding local modifications.${NC}"
    fi
else
    echo -e "${YELLOW}Not a Git repository. Updating current local state.${NC}"
fi

# 2. Build the project
echo -e "\n${BLUE}[2/3] Re-compiling searchxyz in release mode...${NC}"
echo -e "${YELLOW}Running: OPENSSL_VENDORED=1 cargo build --release${NC}"
OPENSSL_VENDORED=1 cargo build --release

# 3. Copy binary
echo -e "\n${BLUE}[3/3] Replacing global user binary...${NC}"
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

cp target/release/searchxyz "$BIN_DIR/searchxyz"
chmod +x "$BIN_DIR/searchxyz"

echo -e "\n${GREEN}=== Update Complete! ===${NC}"
echo -e "Your global installation at ${BLUE}$BIN_DIR/searchxyz${NC} has been updated to the latest build."
