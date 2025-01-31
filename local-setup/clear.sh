#!/usr/bin/env bash

# Print usage information
usage() {
    echo "Usage: $0 [--all]"
    echo
    echo "Options:"
    echo "  --all     Remove all volumes including SSL certificates"
    echo "  --help    Display this help message"
    echo
    exit 1
}

# Parse command line arguments
REMOVE_ALL=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --all)
            REMOVE_ALL=true
            shift
            ;;
        --help)
            usage
            ;;
        *)
            echo "Error: Unknown option: $1"
            usage
            ;;
    esac
done

if [ "$REMOVE_ALL" = true ]; then
    # Remove everything including volumes
    docker compose down -v
else
    # Remove containers but preserve volumes
    docker compose down
    # Remove all volumes except letsencrypt
    docker volume ls --format '{{.Name}}' | grep -v 'local-setup_letsencrypt' | grep 'local-setup_' | xargs -r docker volume rm
fi