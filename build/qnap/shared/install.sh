#!/bin/bash

CONF=/etc/config/qpkg.conf
QPKG_NAME="roon-ai"
QPKG_ROOT=$(/sbin/getcfg $QPKG_NAME Install_Path -f $CONF)

# Check if service was running (for restart after upgrade)
WAS_RUNNING=false
if [ -x "${QPKG_ROOT}/roon-ai.sh" ]; then
    if "${QPKG_ROOT}/roon-ai.sh" status 2>/dev/null | grep -q "running"; then
        WAS_RUNNING=true
    fi
    echo "Stopping existing service for upgrade..."
    "${QPKG_ROOT}/roon-ai.sh" stop 2>/dev/null || true
    sleep 1
fi

# Set executable permissions
chmod +x "${QPKG_ROOT}/roon-ai"
chmod +x "${QPKG_ROOT}/roon-ai.sh"

# Create log file
touch "${QPKG_ROOT}/roon-ai.log"

# Restart service if it was running before upgrade
if [ "$WAS_RUNNING" = true ]; then
    echo "Restarting service after upgrade..."
    "${QPKG_ROOT}/roon-ai.sh" start
fi

echo "Roon AI installed successfully"
echo "Access the web UI at http://$(hostname):8088"

exit 0
