#!/usr/bin/env bash

set -e

# Formatting variables
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== searchxyz Local Installation Script ===${NC}\n"

# 1. Dependency Check
echo -e "${BLUE}[1/4] Checking dependencies...${NC}"
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: Cargo/Rust is not installed. Please install Rust first (https://rustup.rs/).${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Rust/Cargo found.${NC}"

# 2. Build the project
echo -e "\n${BLUE}[2/4] Compiling searchxyz in release mode...${NC}"
echo -e "${YELLOW}Running: OPENSSL_VENDORED=1 cargo build --release${NC}"
OPENSSL_VENDORED=1 cargo build --release

# 3. Create local bin directory and copy binary
echo -e "\n${BLUE}[3/4] Installing binary globally for user...${NC}"
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

cp target/release/searchxyz "$BIN_DIR/searchxyz"
chmod +x "$BIN_DIR/searchxyz"
echo -e "${GREEN}✓ Successfully installed searchxyz to $BIN_DIR/searchxyz${NC}"

# Check if ~/.local/bin is in PATH
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    echo -e "${YELLOW}Warning: $BIN_DIR is not in your PATH.${NC}"
    echo -e "To use searchxyz globally, add this to your ~/.bashrc or ~/.zshrc file:"
    echo -e "  ${BLUE}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"
fi

# 4. Detect and register with Claude Desktop
echo -e "\n${BLUE}[4/4] Configuring Claude Desktop integration...${NC}"
CLAUDE_CONFIG=""
if [[ "$OSTYPE" == "darwin"* ]]; then
    CLAUDE_CONFIG="$HOME/Library/Application Support/Claude/claude_desktop_config.json"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    CLAUDE_CONFIG="$HOME/.config/Claude/claude_desktop_config.json"
fi

if [ -n "$CLAUDE_CONFIG" ] && [ -f "$CLAUDE_CONFIG" ]; then
    echo -e "${GREEN}Found Claude Desktop config at: $CLAUDE_CONFIG${NC}"
    
    # Check if searchxyz is already registered
    if grep -q "searchxyz" "$CLAUDE_CONFIG"; then
        echo -e "${YELLOW}searchxyz is already registered in Claude configuration.${NC}"
    else
        echo -e "${YELLOW}Would you like to automatically register searchxyz in Claude Desktop? (y/n)${NC}"
        read -r response
        if [[ "$response" =~ ^([yY][eE][sS]|[yY])$ ]]; then
            # Backup config
            cp "$CLAUDE_CONFIG" "${CLAUDE_CONFIG}.bak"
            echo -e "${BLUE}Created backup of configuration at ${CLAUDE_CONFIG}.bak${NC}"
            
            # Simple python helper script to inject the JSON key safely
            python3 -c "
import json, sys
path = sys.argv[1]
bin_path = sys.argv[2]
try:
    with open(path, 'r') as f:
        data = json.load(f)
except Exception:
    data = {}
if 'mcpServers' not in data:
    data['mcpServers'] = {}
data['mcpServers']['searchxyz'] = {
    'command': bin_path,
    'args': [],
    'env': {
        'SEARCHXYZ_LOG_LEVEL': 'info'
    }
}
with open(path, 'w') as f:
    json.dump(data, f, indent=2)
" "$CLAUDE_CONFIG" "$BIN_DIR/searchxyz"
            echo -e "${GREEN}✓ Successfully registered searchxyz in Claude Desktop! Please restart Claude to apply.${NC}"
        else
            echo -e "${BLUE}Skipping automatic registration.${NC}"
        fi
    fi
else
    echo -e "${YELLOW}Claude Desktop configuration file not found at default location.${NC}"
    echo -e "You can manually configure searchxyz in your Claude configuration file using:"
    echo -e "${BLUE}"
    cat <<EOF
{
  "mcpServers": {
    "searchxyz": {
      "command": "$BIN_DIR/searchxyz",
      "args": [],
      "env": {
        "SEARCHXYZ_LOG_LEVEL": "info"
      }
    }
  }
}
EOF
    echo -e "${NC}"
fi

echo -e "\n${GREEN}=== Installation Complete! ===${NC}"
echo -e "You can run ${BLUE}searchxyz${NC} from anywhere now (ensure $BIN_DIR is in your PATH)."
