# ZK Thunder Local Setup

A comprehensive local development environment for ZK Thunder, featuring L1/L2 nodes, block explorers, and monitoring tools.

## 🌟 Features

- Layer 1 Development Environment (reth)
- Layer 2 ZK Rollup Infrastructure
- Block Explorers for both L1 and L2
- Monitoring & Development Tools
- Secure Reverse Proxy with Automatic SSL

## 📋 Prerequisites

- Ubuntu Server (the setup script is designed for Ubuntu)
- Root access to the server
- Domain name with Cloudflare DNS account (for SSL certificates)
- 4EVERLAND account with a configured storage bucket
- Wallet with MintLayer (ML) tokens (obtainable from [faucet](https://faucet.mintlayer.org/))

## 🗂️ Project Structure

```
local-setup/
├── blockscout/               # L1 explorer configuration
├── l2-blockexplorer-data/   # L2 explorer data
├── mintlayer-data/          # Mintlayer blockchain data
├── reth_chaindata/          # L1 node data
├── .env                     # Environment configuration
├── .env.example             # Example environment file
├── clear.sh                 # Cleanup script
├── docker-compose.yml       # Main services configuration
├── hyperexplorer.json       # Cross-chain explorer config
├── init.sql                 # Database initialization
├── prometheus.yml           # Monitoring configuration
├── setup.sh                 # Server setup script
└── start.sh                 # Services startup script
```

## 🔧 Configuration

First, create your `.env` file by copying the example file:

```bash
cp .env.example .env
```

Then proceed to configure all the required environment variables as detailed below.

### Environment Variables (.env)

Here's a comprehensive list of environment variables that need to be configured:

#### Domain and Authentication

- `APP_DOMAIN`: Your domain name for the services
- `TRAEFIK_BASIC_AUTH_PASSWORD`: Hashed password for Traefik dashboard
- `GRAFANA_PASSWORD`: Password for Grafana dashboard (default username: admin)

#### Docker Registry Configuration

- `DOCKER_REGISTRY_ACCOUNT`: Your Docker registry account name (e.g., 'dockerhub-username'). This is used to pull custom images for zkthunder and the block explorer

#### Monitoring & Metrics

The setup includes Prometheus and Grafana for monitoring and visualization:

- Prometheus collects metrics from all services every 15 seconds
- Grafana provides dashboards for visualization
- Data is persisted using Docker volumes

Available monitoring endpoint:

- Grafana: <https://grafana.${APP_DOMAIN}>

Default metrics collected from:

- Traefik (port 8080)
- zkthunder (port 3322)
- Explorer API
- Reth node
- PostgreSQL (via postgres-exporter)

#### Database Configuration

- `POSTGRES_USER`: PostgreSQL username
- `POSTGRES_PASSWORD`: PostgreSQL password
- `DB_STRING`: Database connection string

#### Cloudflare Configuration

- `CF_API_EMAIL`: Cloudflare account email
- `CF_DNS_API_TOKEN`: Cloudflare DNS API token
- `CF_ZONE_API_TOKEN`: Cloudflare Zone API token

#### 4EVERLAND Storage

- `4EVERLAND_API_KEY`: API key from 4EVERLAND
- `4EVERLAND_SECRET_KEY`: Secret key from 4EVERLAND
- `4EVERLAND_BUCKET_NAME`: Your bucket name in 4EVERLAND

#### MintLayer Configuration

- `ML_MNEMONIC`: Your wallet mnemonic
- `ML_RPC_USERNAME`: RPC username for MintLayer
- `ML_RPC_PASSWORD`: RPC password for MintLayer
- `ML_TESTNET_NODE_RPC_PASSWORD`: Node RPC password
- `ML_TESTNET_WALLET_RPC_DAEMON_RPC_PASSWORD`: Wallet RPC password

#### Account Allocation and Chain Roles

The L1 chain requires several pre-funded accounts to operate different aspects of the zkThunder system. These accounts must be properly funded in the `reth_config` file's `alloc` section and configured in your environment variables.

##### Chain Roles Configuration

Your `.env` file must configure the following roles, using addresses that are pre-funded in the reth_config:

1. **Operator Account**

   ```
   ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY=<private-key>
   ETH_SENDER_SENDER_OPERATOR_COMMIT_ETH_ADDR=<derived-address>
   ```

   - Responsible for committing transactions and sending data to L1
   - Must be pre-funded in reth_config
   - Both values should be derived from the same private key

2. **Blob Transaction Account**

   ```
   ETH_SENDER_SENDER_OPERATOR_BLOBS_PRIVATE_KEY=<private-key>
   ETH_SENDER_SENDER_OPERATOR_BLOBS_ETH_ADDR=<derived-address>
   ```

   - Handles EIP-4844 blob transactions
   - Must be pre-funded in reth_config
   - Both values should be derived from the same private key

3. **Fee Account**

   ```
   CHAIN_STATE_KEEPER_FEE_ACCOUNT_ADDR=<address>
   FEE_RECEIVER_PRIVATE_KEY=<private-key>
   ```

   - Collects transaction fees
   - Must be pre-funded in reth_config
   - Both values should be derived from the same private key

4. **Governance Accounts**

   ```
   DEPLOYER_PRIVATE_KEY=<private-key>
   GOVERNOR_PRIVATE_KEY=<private-key>
   GOVERNOR_ADDRESS=<derived-address>
   CHAIN_ADMIN_ADDRESS=<derived-address>
   ```

   - Basic setup: Use the same private key for DEPLOYER and GOVERNOR roles
   - Advanced setup: Can use different addresses for granular permissions
   - All addresses must be pre-funded in reth_config
   - GOVERNOR_ADDRESS and CHAIN_ADMIN_ADDRESS should be derived from their respective private keys

#### L1 Chain Configuration

The Layer 1 chain configuration is defined in `local-setup/reth_chaindata/reth_config`. This file configures the local Ethereum-compatible chain used as the L1 for the zkSync deployment.

##### Chain Parameters

- `chainId`: 9 - Unique identifier for this development chain
- `gasLimit`: "0x1c9c380" (~30M gas) - Maximum gas per block
- `baseFeePerGas`: 1 - Minimal base fee for EIP-1559 transactions
- Proto-danksharding parameters:
  - `excessBlobGas`: "0x0"
  - `blobGasUsed`: 0

##### Network Configuration

- All major Ethereum upgrades (Homestead, EIP150, EIP155, DAO Fork, Frontier, Byzantium, Constantinople, Petersburg, Muir Glacier, Istanbul, Berlin, London, Shanghai, Cancun) are activated from block 0
- Uses Clique PoA consensus with 0 period (instant blocks) and 30000 epoch length
- Terminal Total Difficulty is set to 0 (post-merge from start)

##### Important Notes

1. All addresses used in your environment configuration MUST be pre-funded in the reth_config `alloc` section
2. Each role requires sufficient ETH for gas fees and operations
3. When generating new keys for these roles, make sure to:
   - Add the derived addresses to reth_config's `alloc` section
   - Update your .env file with the corresponding private keys and addresses
   - Keep your private keys secure and never commit them to version control

For development environments, you can use addresses from the pre-funded rich wallets list. The primary rich wallet (default: `0x36615Cf349d7F6344891B1e7CA7C72883F5dc049`) is particularly important as it receives initial allocations of any deployed ERC-20 tokens during system bootstrapping.

### HyperExplorer Configuration

The `hyperexplorer.json` file configures the cross-chain explorer that monitors communication between Layer 1 and Layer 2. You must manually edit this file and replace the placeholder variables with their actual values.

Example configuration (before manual replacement):

```json
{
    "networks": {
        "local": {
            "l1_url": "http://reth:8545",
            "explorer_prefix": "https://l1explorer.${APP_DOMAIN}/",
            "explorer_address_prefix": "https://l1explorer.${APP_DOMAIN}/address/",
            "single_bridges": {},
            "shared_bridges": {
                "kl_exp": {
                    "chains": {
                        "zkthunder": {
                            "chain_id": "${L2_EXPLORER_CHAIN_ID_IN_BASE16}",
                            "l2_url": "http://zkthunder:3050",
                            "explorer": "https://l2explorer.${APP_DOMAIN}/",
                            "type": "rollup"
                        }
                    }
                }
            }
        }
    }
}
```

Required manual replacements:

1. Replace `${APP_DOMAIN}` with your actual domain name (e.g., if APP_DOMAIN=example.com in your .env, replace all instances with "example.com")
2. Replace `${L2_EXPLORER_CHAIN_ID_IN_BASE16}` with your L2 chain ID in hexadecimal format (e.g., "0x108D")

Configuration fields:

- `l1_url`: Layer 1 RPC endpoint
- `explorer_prefix`: Base URL for L1 explorer
- `explorer_address_prefix`: URL pattern for L1 addresses
- `shared_bridges.kl_exp.chains.zkthunder`:
  - `chain_id`: Network identifier in BASE16 format
  - `l2_url`: Layer 2 RPC endpoint
  - `explorer`: Base URL for L2 explorer
  - `type`: Always "rollup" for ZK Thunder

### Blockscout Frontend Configuration

The L1 block explorer frontend requires additional configuration in `local-setup/blockscout/common-frontend.env`. You need to manually set the following variables:

```bash
# L1 Explorer Frontend Environment Variables
NEXT_PUBLIC_API_HOST=https://l1api.${APP_DOMAIN}
NEXT_PUBLIC_APP_HOST=https://l1explorer.${APP_DOMAIN}
NEXT_PUBLIC_VISUALIZE_API_HOST=https://l1api.${APP_DOMAIN}
```

Replace `${APP_DOMAIN}` with your actual domain name (e.g., if your domain is example.com, the values would be <https://l1api.example.com>, etc.).

These variables configure the various endpoints that the Blockscout frontend uses to:

- Connect to the API server
- Fetch statistics
- Set the application host
- Access visualization features

### Customizing Frontend Services

The L2 block explorer frontend service requires customization for your domain:

- Block Explorer (`explorer-app`)

#### Building Custom Images

1. Clone the repository:

   ```bash
   git clone https://github.com/matter-labs/block-explorer.git
   cd block-explorer/packages/app
   ```

2. Edit the environment configuration (remeber to change ${APP_DOMAIN} with your actual domain):

   ```json
   // Block Explorer (src/configs/dev.config.json)
   {
     "networks": [
       {
         "apiUrl": "https://l2api.${APP_DOMAIN}",
         "verificationApiUrl": "https://l2api.${APP_DOMAIN}",
         "hostnames": [],
         "icon": "/images/icons/zksync-arrows.svg",
         "l1ExplorerUrl": "https://l1explorer.${APP_DOMAIN}",
         "l2ChainId": 4237,
         "l2NetworkName": "ZkThunder",
         "maintenance": false,
         "name": "zkthunder",
         "published": true,
         "rpcUrl": "https://rpc.${APP_DOMAIN}",
         "baseTokenAddress": "0x000000000000000000000000000000000000800A"
       }
     ]
   }

   ```

3. Build and push Docker images:

   ```bash
   # Build image
   docker build -t ${DOCKER_REGISTRY_ACCOUNT}/zk-explorer:zkthunder .

   # Push to registry
   docker push ${DOCKER_REGISTRY_ACCOUNT}/zk-explorer:zkthunder
   ```

   Note: Make sure the `DOCKER_REGISTRY_ACCOUNT` in your .env file matches your Docker registry username where you have push permissions.

4. The docker-compose.yml will automatically use your registry account as configured in the .env file to pull the images:

   ```yaml
   explorer-app:
     image: ${DOCKER_REGISTRY_ACCOUNT}/zk-explorer:zkthunder
   ```

### How to Source Required Information

#### Basic Auth Setup

Generate Traefik's basic auth password:

1. Install and use htpasswd:

   ```bash
   htpasswd -nb admin your-password
   ```

2. In the resulting string, double every dollar sign ($). For example:

   ```bash
   # Original htpasswd output:
   admin:$apr1$ruca84Hq$mbjdMZBAG.KWn7vfN/SNK/

   # Modified for .env file (note the doubled $ signs):
   TRAEFIK_BASIC_AUTH_PASSWORD=admin:$$apr1$$ruca84Hq$$mbjdMZBAG.KWn7vfN/SNK/
   ```

#### Domain Setup

Set `APP_DOMAIN` to your domain name (e.g., mydomain.com) in two places:

1. In your `.env` file:

   ```bash
   APP_DOMAIN=mydomain.com
   ```

2. In `start.sh`:

   ```bash
   APP_DOMAIN='mydomain.com'   # Must match APP_DOMAIN in .env
   ```

All services will be accessible as subdomains of this domain.

#### Cloudflare Setup

1. Log into your Cloudflare Dashboard
2. Navigate to "API Tokens"
3. Create two tokens:
   - One with DNS:Edit permission → `CF_DNS_API_TOKEN`
   - One with Zone:Read permission → `CF_ZONE_API_TOKEN`
4. Set `CF_API_EMAIL` to your Cloudflare account email

#### 4EVERLAND Setup

1. Create an account at [4EVERLAND](https://4everland.org/)
2. Navigate to Dashboard → Storage → Bucket
3. Create a new bucket
4. Go to Account → API Keys to generate:
   - Copy API key to `4EVERLAND_API_KEY`
   - Copy Secret key to `4EVERLAND_SECRET_KEY`
5. Set `4EVERLAND_BUCKET_NAME` to your bucket name

#### MintLayer Setup

1. Visit [MintLayer Faucet](https://faucet.mintlayer.org/)
2. Request tokens for your wallet address
3. Set secure passwords for:
   - `ML_TESTNET_NODE_RPC_PASSWORD`
   - `ML_TESTNET_WALLET_RPC_DAEMON_RPC_PASSWORD`

### Deploying L2 Faucet

You can deploy a faucet contract on the L2 network to allow users to request test ETH. Here's how to set it up:

1. First, configure the environment:

   ```bash
   cd local-setup-test
   cp .env.example .env
   ```

2. Edit the `.env` file to set these required variables:

   ```bash
   MAIN_URI=your-domain.com    # Your domain without protocol (e.g., example.com)
   RICH_WALLET_PK=your-private-key    # Private key of a wallet with sufficient ETH
   ```

3. Deploy the faucet contract:

   ```bash
   npx hardhat run ./scripts/deploy-faucet.ts
   ```

   The deployment script will:
   - Use the specified rich wallet to deploy the contract
   - Configure the faucet with:
     - Maximum 10 transactions per hour per address
     - 24-hour time limit between requests (86400 seconds)
   - Auto-fund the deployer wallet if needed
   - Output the deployed contract address

4. Optional: Verify the contract on the L2 explorer:

   ```bash
   yarn hardhat verify <DEPLOYED_CONTRACT_ADDRESS> 10 86400
   ```

   Where:
   - `<DEPLOYED_CONTRACT_ADDRESS>`: The address output from the deployment script
   - `10`: Maximum transactions per hour parameter
   - `86400`: Time limit in seconds parameter

After verification, users can interact with the faucet directly through the L2 explorer interface at `https://l2explorer.${APP_DOMAIN}`.

## 🚀 Getting Started

1. Setup the server (optional hardening):

   ```bash
   sudo ./setup.sh [--default]
   ```

   Options:
   - No flag: Interactive mode, asks confirmation for each component
   - `--default`: Non-interactive mode, installs all components automatically
   - `--help`: Displays help message

2. Create and configure your environment file:

   ```bash
   cp .env.example .env
   # Edit .env with your configurations following the Configuration section below
   ```

3. Start the services:

   ```bash
   ./start.sh
   ```

4. Clean up when needed:

   ```bash
   ./clear.sh
   ```

## 🌐 Services

### Core Infrastructure

| Service    | URL                          | Description                  |
|------------|------------------------------|------------------------------|
| Traefik    | traefik.${APP_DOMAIN}       | Reverse Proxy & SSL         |
| PostgreSQL | -                           | Database                     |
| PgAdmin    | pgadmin.${APP_DOMAIN}       | Database Management UI      |
| Prometheus | -                           | Metrics Collection           |
| Grafana    | grafana.${APP_DOMAIN}       | Monitoring Dashboard        |

### Layer 1 Services

| Service           | URL                          | Description              |
|------------------|------------------------------|--------------------------|
| Reth Node        | reth.${APP_DOMAIN}          | L1 Node RPC Endpoint    |
| L1 Explorer      | l1explorer.${APP_DOMAIN}    | Block Explorer          |
| L1 Explorer API  | l1api.${APP_DOMAIN}         | Explorer API            |

### Layer 2 Services

| Service           | URL                          | Description              |
|------------------|------------------------------|--------------------------|
| ZK Thunder Node  | rpc.${APP_DOMAIN}           | L2 Node RPC             |
| WebSocket        | ws.${APP_DOMAIN}            | WebSocket Endpoint      |
| L2 Explorer      | l2explorer.${APP_DOMAIN}    | Block Explorer          |
| L2 Explorer API  | l2api.${APP_DOMAIN}         | Explorer API            |
| Health Check     | health.${APP_DOMAIN}        | Node Health Status      |
| HyperExplorer    | hyperexplorer.${APP_DOMAIN} | Cross-chain Explorer    |

## 🔐 Security Features

The setup includes several security features:

- Automatic SSL certificate generation
- Basic authentication for admin interfaces
- Rate limiting for RPC endpoints
- Fail2Ban integration
- UFW firewall configuration
- SSH hardening
- System auditing (auditd)
- Secure sysctl parameters

## 📜 Logs

Logs are available through Docker Compose:

```bash
docker compose logs -f [service-name]
```

## 🧹 Cleanup

To clean up your environment, use the clear script:

```bash
./clear.sh [--all]
```

Options:

- No flag: Removes all containers and volumes except SSL certificates
- `--all`: Removes everything including SSL certificates (uses docker compose down -v)
- `--help`: Displays help message

Preserving SSL certificates (default behavior) is useful for faster redeployment since you won't need to regenerate them.
