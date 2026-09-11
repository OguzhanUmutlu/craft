#!/bin/bash
set -e

# Setup Craft home directory permissions if mounted from host
CRAFT_DATA_DIR="${CRAFT_HOME:-/craft}"
mkdir -p "$CRAFT_DATA_DIR"

if [ "$(id -u)" = '0' ]; then
    chown -R craft:craft "$CRAFT_DATA_DIR"
    EXEC_CMD="gosu craft"
else
    EXEC_CMD=""
fi

# If no arguments provided, start the Craft service daemon in foreground
if [ $# -eq 0 ] || [ "$1" = 'daemon' ] || [ "$1" = 'service' ]; then
    echo "================================================================"
    echo "  Starting Craft 2.0 Background Supervisor Daemon"
    echo "  Data Directory: $CRAFT_DATA_DIR"
    echo "================================================================"
    exec $EXEC_CMD craft service start --foreground
fi

# If first argument starts with '-' or is a known craft subcommand, prepend 'craft'
if [ "${1:0:1}" = '-' ]; then
    exec $EXEC_CMD craft "$@"
fi

case "$1" in
    new|run|stop|view|ls|rm|load|ver|update|cache|service|auto|fix|plugin|ping|rcon|backup|firewall|loopback|template|dockerize|remote|deploy)
        exec $EXEC_CMD craft "$@"
        ;;
    *)
        exec $EXEC_CMD "$@"
        ;;
esac
