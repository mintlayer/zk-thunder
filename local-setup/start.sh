#!/usr/bin/env bash
# docker compose up -d

# usage: ./start.sh INSTANCE_TYPE 
# Instance type is specifying the docker image to take:
# see https://hub.docker.com/r/matterlabs/local-node/tags for full list.
# latest2.0 - is the 'main' one.

INSTANCE_TYPE=${1:-zkthunder}
APP_DOMAIN='zkthunder.fi'

export INSTANCE_TYPE=$INSTANCE_TYPE

# Ensure zkthunder-data directory exists and create dev.env if it doesn't exist
mkdir -p zkthunder-data
if [ ! -f zkthunder-data/dev.env ]; then
  touch zkthunder-data/dev.env
  echo "Created empty dev.env file in zkthunder-data directory."
else
  echo "dev.env file already exists in zkthunder-data directory."
fi

echo "Starting zkthunder with instance type: $INSTANCE_TYPE"
docker compose pull --ignore-pull-failures
# docker compose up 
docker compose up -d

check_all_services_healthy() {
  service="zkthunder"
  # service="zksync"
  (docker compose ps $service | grep "(healthy)")
  if [ $? -eq 0 ]; then
    return 0
  else
    return 1  # If any service is not healthy, return 1
  fi
}

# Loop until all services are healthy
while ! check_all_services_healthy; do
  echo "Services are not yet healthy, waiting..."
  sleep 10  # Check every 10 seconds
done

echo "All services are healthy!"

DEFAULT='\033[0;29m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
DARKGRAY='\033[0;30m'
ORANGE='\033[0;33m'
echo -e "${GREEN}"

echo -e "SUCCESS, Your local zkthunder is now running! Find the information below for accessing each service."
echo -e "┌─────────────────────────┬────────────────────────────────────────┬──────────────────────────────────────────────────┐"
echo -e "│         Service         │              URL                       │                   Description                    │"
echo -e "├─────────────────────────┼────────────────────────────────────────┼──────────────────────────────────────────────────┤"
echo -e "│ ${ORANGE}Traefik Dashboard       ${GREEN}│ ${BLUE}https://traefik.${APP_DOMAIN}${GREEN}           │ ${DARKGRAY}Traefik Dashboard and API                        ${GREEN}│"
echo -e "│ ${ORANGE}PgAdmin                 ${GREEN}│ ${BLUE}https://pgadmin.${APP_DOMAIN}${GREEN}           │ ${DARKGRAY}UI to manage the PostgreSQL databases            ${GREEN}│"
echo -e "│ ${ORANGE}Grafana                 ${GREEN}│ ${BLUE}https://grafana.${APP_DOMAIN}${GREEN}           │ ${DARKGRAY}Service metrics visualization and monitoring     ${GREEN}│"
echo -e "│ ${ORANGE}L1 RPC (reth)           ${GREEN}│ ${BLUE}https://reth.${APP_DOMAIN}${GREEN}              │ ${DARKGRAY}HTTP Endpoint for L1 reth node                   ${GREEN}│"
echo -e "│ ${ORANGE}L1 Explorer             ${GREEN}│ ${BLUE}https://l1explorer.${APP_DOMAIN}${GREEN}        │ ${DARKGRAY}Block Explorer for Layer 1                       ${GREEN}│"
echo -e "│ ${ORANGE}L1 Explorer API         ${GREEN}│ ${BLUE}https://l1api.${APP_DOMAIN}${GREEN}             │ ${DARKGRAY}API for L1 Block Explorer                        ${GREEN}│"
echo -e "│ ${ORANGE}L2 RPC                  ${GREEN}│ ${BLUE}https://rpc.${APP_DOMAIN}${GREEN}               │ ${DARKGRAY}HTTP RPC Endpoint for L2                         ${GREEN}│"
echo -e "│ ${ORANGE}L2 WebSocket            ${GREEN}│ ${BLUE}wss://ws.${APP_DOMAIN}${GREEN}                  │ ${DARKGRAY}WebSocket Endpoint for L2                        ${GREEN}│"
echo -e "│ ${ORANGE}L2 Health Check         ${GREEN}│ ${BLUE}https://health.${APP_DOMAIN}${GREEN}            │ ${DARKGRAY}Health Check Endpoint for L2                     ${GREEN}│"
echo -e "│ ${ORANGE}L2 Explorer             ${GREEN}│ ${BLUE}https://l2explorer.${APP_DOMAIN}${GREEN}        │ ${DARKGRAY}Block Explorer for L2                            ${GREEN}│"
echo -e "│ ${ORANGE}L2 Explorer API         ${GREEN}│ ${BLUE}https://l2api.${APP_DOMAIN}${GREEN}             │ ${DARKGRAY}API for L2 Block Explorer                        ${GREEN}│"
echo -e "│ ${ORANGE}L2 Explorer Metrics     ${GREEN}│ ${BLUE}https://l2metrics.${APP_DOMAIN}${GREEN}         │ ${DARKGRAY}Metrics for L2 Block Explorer                    ${GREEN}│"
echo -e "│ ${ORANGE}HyperExplorer           ${GREEN}│ ${BLUE}https://hyperexplorer.${APP_DOMAIN}${GREEN}     │ ${DARKGRAY}Explorer for communication between ZK Chains     ${GREEN}│"
echo -e "└─────────────────────────┴────────────────────────────────────────┴──────────────────────────────────────────────────┘"

echo -e "${DEFAULT}"