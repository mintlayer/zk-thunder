import { config as dotenvConfig } from 'dotenv';
import { HardhatUserConfig } from 'hardhat/config';

dotenvConfig();

require('@matterlabs/hardhat-zksync-deploy');
require('@matterlabs/hardhat-zksync-solc');
require('@matterlabs/hardhat-zksync-verify');

const mainUri = process.env.MAIN_URI;

// dynamically changes endpoints for local tests
const zkSyncTestnet =
    process.env.NODE_ENV == 'test'
        ? {
              url: 'http://localhost:15100',
              ethNetwork: 'http://127.0.0.1:15045',
              zksync: true
          }
        : process.env.NODE_ENV == 'development'
        ? {
              url: 'http://localhost:25100',
              ethNetwork: 'http://localhost:25045',
              zksync: true
          }
        : process.env.NODE_ENV == 'local'
        ? {
              url: 'http://localhost:3050',
              ethNetwork: 'http://localhost:8545',
              zksync: true
          }
        : {
            url: `https://rpc.${mainUri}`,
            ethNetwork: `https://reth.${mainUri}`,
            zksync: true,
            // contract verification endpoint
            verifyURL: `https://l2api.${mainUri}/contract_verification`
          };

const config: HardhatUserConfig = {
    zksolc: {
        version: '1.3.22',
        settings: {}
    },
    defaultNetwork: 'zkSyncTestnet',
    networks: {
        hardhat: {
            // @ts-ignore
            zksync: true
        },
        zkSyncTestnet
    },
    solidity: {
        version: '0.8.17'
    }
};

export default config;
