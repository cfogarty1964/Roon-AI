#!/bin/bash
# Post-installation script for roon-ai

# Reload systemd
systemctl daemon-reload

# Enable the service
systemctl enable roon-ai.service

# Restart (not start) to handle upgrades - loads new binary
systemctl restart roon-ai.service

echo "Roon AI installed successfully!"
echo "Service is running on http://localhost:8088"
