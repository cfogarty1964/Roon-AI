#!/bin/bash

# Uninstall script for Roon AI

set -e

# Check for root privileges
if [[ $EUID -ne 0 ]]; then
    echo "This script requires administrator privileges."
    echo "Please run with: sudo $0"
    exit 1
fi

echo "Uninstalling Roon AI..."

# Stop and unload the service
launchctl stop com.221b.roon-ai 2>/dev/null || true
launchctl unload /Library/LaunchDaemons/com.221b.roon-ai.plist 2>/dev/null || true

# Remove files
rm -f /usr/local/bin/roon-ai
rm -f /Library/LaunchDaemons/com.221b.roon-ai.plist

# Handle configuration data removal
# In non-interactive mode, preserve config by default
if [[ -t 0 ]]; then
    # Interactive mode - ask user
    read -p "Remove configuration data? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf /usr/local/var/roon-ai
        echo "Configuration data removed."
    else
        echo "Configuration data preserved at /usr/local/var/roon-ai"
    fi
else
    # Non-interactive mode - preserve config
    echo "Non-interactive mode: configuration data preserved at /usr/local/var/roon-ai"
    echo "To remove manually: sudo rm -rf /usr/local/var/roon-ai"
fi

# Remove package receipt
pkgutil --forget com.221b.roon-ai 2>/dev/null || true

echo "Roon AI has been uninstalled."
