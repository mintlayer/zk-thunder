import { Wallet, Contract, utils } from 'zksync-web3';
import * as hre from 'hardhat';
import { ethers } from 'ethers';
import { Deployer } from '@matterlabs/hardhat-zksync-deploy';
import { config as dotenvConfig } from 'dotenv';

dotenvConfig();

const RICH_WALLET_PK = [process.env.RICH_WALLET_PK];

async function deployFaucet(
  deployer: Deployer, 
  maxTxPerHour: number, 
  timeLimit: number
): Promise<Contract> {
  try {
    console.log('Deploying Faucet contract');
    const artifact = await deployer.loadArtifact('Faucet');
    const faucet = await deployer.deploy(artifact, [maxTxPerHour, timeLimit]);
    
    console.log(`Faucet contract deployed to address: ${faucet.address}`);
    
    return faucet;
  } catch (error) {
    console.error('Error deploying Faucet contract');
    console.error(error);
    throw error;
  }
}

async function main() {
  const maxTxPerHour = 10;
  const timeLimit = 86400; // 24 hours in seconds
  
  const deployer = new Deployer(hre, new Wallet(RICH_WALLET_PK[0]));

  console.log(`Using deployer wallet: ${deployer.zkWallet.address}`);
  console.log(`Deploying with parameters:
    - Max transactions per hour: ${maxTxPerHour}
    - Time limit: ${timeLimit} seconds`);

  // Optional: Fund the wallet if needed
  const depositAmount = ethers.utils.parseEther('0.1');
  const depositHandle = await deployer.zkWallet.deposit({
    to: deployer.zkWallet.address,
    token: utils.ETH_ADDRESS,
    amount: depositAmount
  });
  await depositHandle.wait();

  console.log('Funding complete. Deploying contract...');
  
  await deployFaucet(deployer, maxTxPerHour, timeLimit);
}

main()
  .then(() => {
    console.log('Deployment script completed successfully.');
    process.exit(0);
  })
  .catch((error) => {
    console.error('Deployment script encountered an error:', error);
    process.exit(1);
  });