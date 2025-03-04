import { utils } from 'zksync-ethers';
import { ethers } from 'ethers';
import * as fs from 'fs';
import * as path from 'path';
import { ValidatorTimelockFactory } from '../../../contracts/l1-contracts/typechain/ValidatorTimelockFactory';
import { StateTransitionManagerFactory } from '../../../contracts/l1-contracts/typechain/StateTransitionManagerFactory';

interface WalletKey {
    address: string;
    privateKey: string;
}

// Cache object for storing contract instances
const contractCache = {
    contractMain: null,
    stm: null,
    validatorTimelock: null
};

// Function to get contract instances
async function getContractInstances() {
    const ethProvider = getEthersProvider();

    if (!contractCache.contractMain) {
        contractCache.contractMain = new ethers.Contract(
            process.env.CONTRACTS_DIAMOND_PROXY_ADDR,
            utils.ZKSYNC_MAIN_ABI,
            ethProvider
        );
    }

    if (!contractCache.stm) {
        const stateTransitionManagerAddr = await contractCache.contractMain.getStateTransitionManager();
        contractCache.stm = StateTransitionManagerFactory.connect(stateTransitionManagerAddr, ethProvider);
    }

    if (!contractCache.validatorTimelock) {
        const validatorTimelockAddr = await contractCache.stm.validatorTimelock();
        contractCache.validatorTimelock = ValidatorTimelockFactory.connect(validatorTimelockAddr, ethProvider);
    }

    return {
        contractMain: contractCache.contractMain,
        stm: contractCache.stm,
        validatorTimelock: contractCache.validatorTimelock
    };
}

export async function isOperator(chainId: string, walletAddress: string): Promise<boolean> {
    try {
        const { validatorTimelock } = await getContractInstances();
        const isOperator = await validatorTimelock.validators(chainId, walletAddress);
        return isOperator;
    } catch (error) {
        console.error('Error checking if address is an operator:', error);
        throw error;
    }
}

export function getWalletKeys(): WalletKey[] {
    // Use private keys from environment variables
    const privateKeys = process.env.API_WEB3_JSON_RPC_ACCOUNT_PKS
        ? process.env.API_WEB3_JSON_RPC_ACCOUNT_PKS.split(',').map(key => key.trim())
        : [];
    
    // If no keys are provided in environment, log a warning
    if (privateKeys.length === 0) {
        console.warn('No account private keys found in API_WEB3_JSON_RPC_ACCOUNT_PKS environment variable');
    }

    const walletKeys: WalletKey[] = [];
    for (const privateKey of privateKeys) {
        const wallet = new ethers.Wallet(privateKey);
        walletKeys.push({
            address: wallet.address,
            privateKey: privateKey
        });
    }
    
    return walletKeys;
}

export function getEthersProvider(): ethers.providers.JsonRpcProvider {
    return new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL || 'http://localhost:8545');
}
