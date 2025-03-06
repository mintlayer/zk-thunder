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
- `BATCH_SIZE`: The number of IPFS CIDs to consolidate into a single IPFS CID batch, should be a multiple of 3

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
   GOVERNANCE_PRIVATE_KEY=<private-key>
   ```

   - Basic setup: Use the same private key for DEPLOYER, GOVERNOR and GOVERNANCE roles
   - Advanced setup: Can use different addresses for granular permissions
   - All addresses must be pre-funded in reth_config
   - GOVERNOR_ADDRESS and CHAIN_ADMIN_ADDRESS should be derived from their respective private keys

5. **Validator Account**

   ```
   VALIDATOR_PRIVATE_KEY=<private-key>
   ```

   - Responsible for validating transactions and blocks
   - Must be pre-funded in reth_config

6. **Pre-funded Account**

   ```
   PRE_FUNDED_ACCOUNT_PKS=<private-key>
   ```

   - Pre-funded account private keys

#### L1 Chain Configuration

The Layer 1 chain configuration is defined in `local-setup/reth_chaindata/reth_config`. This file configures the local Ethereum-compatible chain used as the L1 for the zkSync deployment.

##### L1 Chain Parameters

- `chainId`: 9 - Unique identifier for this development chain
- `gasLimit`: "0x1c9c380" (~30M gas) - Maximum gas per block
- `baseFeePerGas`: 1 - Minimal base fee for EIP-1559 transactions
- Proto-danksharding parameters:
  - `excessBlobGas`: "0x0"
  - `blobGasUsed`: 0

##### Network Configuration

- All major Ethereum upgrades (Homestead, EIP150, EIP155, DAO Fork, Frontier, Byzantium, Constantinople, Petersburg, Muir Glacier, Istanbul, Berlin, London, Shanghai, Cancun) are activated from block 0
- Uses Clique PoA consensus with 6s period and 30000 epoch length
- Terminal Total Difficulty is set to 0 (post-merge from start)

##### Configuring L1 Block Time

The L1 block time can be configured in the `docker-compose.yml` file through the Reth node service configuration. By default, it's set to 6 seconds (6000ms):

```yaml
reth:
  command: >
    node --metrics 0.0.0.0:9001 
    --dev 
    --datadir /rethdata 
    --http 
    --http.corsdomain "*" 
    --http.addr 0.0.0.0 
    --http.port 8545
    --dev.block-time 6000ms
    --chain /chaindata/reth_config
```

To adjust the block time:

1. Modify the `--dev.block-time` parameter in the `reth` service command
2. Values are specified in milliseconds (e.g., `6000ms` for 6 seconds)
3. Shorter block times (e.g., `1000ms`) will result in faster L1 block production but may increase resource usage
4. Longer block times (e.g., `12000ms`) will reduce resource usage but slow down L1 transaction confirmations

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

## ⛓️ Chain Configuration

### Block and Batch Structure

ZK Thunder uses a hierarchical structure to process transactions:

1. **Transactions**: Individual user operations
2. **L2 Blocks (Miniblocks)**: Collections of transactions processed together
3. **L1 Batches**: Groups of L2 blocks that are proven and committed to L1

### Block Times and Batch Sizes

#### L2 Block Configuration

- **L2 Block Commit Deadline**: `CHAIN_STATE_KEEPER_MINIBLOCK_COMMIT_DEADLINE_MS=1000` (1 second)
  - Controls how frequently L2 blocks are created
  - New L2 blocks are sealed either when this time passes or when other criteria are met

- **L2 Block Max Payload Size**: `CHAIN_STATE_KEEPER_MINIBLOCK_MAX_PAYLOAD_SIZE=1000000` (1MB)
  - Maximum size of an L2 block's payload in bytes
  - Blocks will be sealed when they approach this limit

#### L1 Batch Configuration

- **L1 Batch Commit Deadline**: `CHAIN_STATE_KEEPER_BLOCK_COMMIT_DEADLINE_MS=30000` (30 seconds)
  - Maximum time before an L1 batch is unconditionally sealed
  - Critical for controlling the timing of proof generation and L1 commitments

- **Transaction Slots**: `CHAIN_STATE_KEEPER_TRANSACTION_SLOTS=250`
  - Maximum number of transactions that can be included in an L1 batch
  - Batches will be sealed when this limit is reached

- **Max Gas Per Batch**: `CHAIN_STATE_KEEPER_MAX_GAS_PER_BATCH=200000000` (200M)
  - Maximum amount of gas that can be used in a single L1 batch
  - Derived from circuit limitations per batch

- **Max Pubdata Per Batch**: `CHAIN_STATE_KEEPER_MAX_PUBDATA_PER_BATCH=100000` (100KB)
  - Maximum amount of pubdata (in bytes) that can be published per batch
  - Affects blob usage: <126KB uses 1 blob, 126-252KB uses 2 blobs

- **Max Circuits Per Batch**: `CHAIN_STATE_KEEPER_MAX_CIRCUITS_PER_BATCH=24100`
  - Maximum number of circuits that a batch can support
  - Refers to "base layer" circuits, not including recursion layers

#### Sealing Criteria

Blocks and batches can be sealed (closed for new transactions) based on several criteria:

- **Geometry Percentage**: Controls sealing based on payload size
  - `CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_GEOMETRY_PERCENTAGE=0.95` (95%)
  - `CHAIN_STATE_KEEPER_REJECT_TX_AT_GEOMETRY_PERCENTAGE=0.95` (95%)

- **Gas Percentage**: Controls sealing based on gas usage
  - `CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_GAS_PERCENTAGE=0.95` (95%)
  - `CHAIN_STATE_KEEPER_REJECT_TX_AT_GAS_PERCENTAGE=0.95` (95%)

- **ETH Params Percentage**: Controls sealing based on L1 parameters
  - `CHAIN_STATE_KEEPER_CLOSE_BLOCK_AT_ETH_PARAMS_PERCENTAGE=0.95` (95%)
  - `CHAIN_STATE_KEEPER_REJECT_TX_AT_ETH_PARAMS_PERCENTAGE=0.95` (95%)

### Fee Model Configuration

- **Fee Model Version**: `CHAIN_STATE_KEEPER_FEE_MODEL_VERSION=V2`
  - V2 allows pubdata price to be independent from L1 gas price
  - L2 gas price includes both proving/computation costs and L1 batch processing costs

- **Minimal L2 Gas Price**: `CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE=100000000` (100 Gwei)
  - Minimum acceptable gas price for L2 transactions
  - Includes cost of computation/proving and congestion premium

- **Overhead Configuration**:
  - `CHAIN_STATE_KEEPER_COMPUTE_OVERHEAD_PART=0.0` (0%)
  - `CHAIN_STATE_KEEPER_PUBDATA_OVERHEAD_PART=1.0` (100%)
  - `CHAIN_STATE_KEEPER_BATCH_OVERHEAD_L1_GAS=800000` (800K gas)

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


#### Chain State Keeper Configuration
- `CHAIN_STATE_KEEPER_BLOCK_COMMIT_DEADLINE_MS`: This variable defines the maximum time limit, in milliseconds, for the batch commit process to Layer 1

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

## 🚀 Deploying Changes to Production

When you need to apply changes to the production environment, you'll need to follow a specific workflow to ensure your changes are properly built, tagged, and deployed.

### Step-by-Step Deployment Guide

1. **Make and Test Your Changes Locally**
   - Develop and test your changes in a local development environment
   - Ensure all tests pass and the application works as expected

2. **Create and Push a Release Tag**
   - The tag format must follow the pattern: `vX.Y.Z-zkthunder`
   - Example: `v1.2.3-zkthunder`

   ```bash
   # Create the tag
   git tag v1.2.3-zkthunder
   
   # Push the tag to the remote repository
   git push origin v1.2.3-zkthunder
   ```

3. **Wait for CI/CD Pipeline**
   - The CI/CD pipeline will automatically:
     - Detect the new tag
     - Build the Docker image
     - Push the image to DockerHub with the appropriate tag
   - You can monitor the build progress in your CI/CD dashboard

4. **Deploy the New Image to Production**
   - SSH into your production server
   - Navigate to your deployment directory

   ```bash
   cd /path/to/local-setup
   ```

   - Pull the latest Docker image

   ```bash
   docker pull ${DOCKER_REGISTRY_ACCOUNT}/<service-name>:zkthunder
   ```

   - Restart the affected service(s)

   ```bash
   # To restart a specific service
   docker compose up -d <service-name>
   
   # Or to pull and restart all services if needed
   ./start.sh
   ```

5. **Verify the Deployment**
   - Check that the service is running with the new version

   ```bash
   docker compose ps
   ```

   - Verify the application is functioning correctly by accessing it through your browser
   - Check the logs for any errors

   ```bash
   docker compose logs -f <service-name>
   ```

### Important Notes

- **Version Numbering**: Follow semantic versioning (X.Y.Z):
  - X: Major version (breaking changes)
  - Y: Minor version (new features, no breaking changes)
  - Z: Patch version (bug fixes)

- **Tag Format**: The `-zkthunder` suffix is required for the CI/CD pipeline to recognize it as a production release

- **Rollback Procedure**: If issues are detected, you can roll back to a previous version:

  ```bash
  # Pull the previous working version
  docker pull ${DOCKER_REGISTRY_ACCOUNT}/<service-name>:previous-tag
  
  # Update your docker-compose.yml to use the previous version
  # Then restart the service
  docker compose up -d --no-deps <service-name>
  ```

- **Environment Variables**: Ensure any new environment variables required by your changes are properly set in the production environment's `.env` file

By following this deployment workflow, you can ensure smooth and consistent updates to your production environment.