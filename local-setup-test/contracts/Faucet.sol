// SPDX-License-Identifier: MIT
pragma solidity ^0.8.11;

import "@openzeppelin/contracts/access/Ownable2Step.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/security/Pausable.sol";

/**
 * @title Faucet
 * @dev A simple faucet contract that dispenses ETH to requesters with rate limits.
 */
contract Faucet is Ownable2Step, ReentrancyGuard, Pausable {
    // Constants
    uint256 public constant DISPENSE_AMOUNT = 0.1 ether;
    
    // State variables
    uint256 public maxTxPerHour;
    uint256 public timeLimit;
    
    // User request tracking
    mapping(address => uint256) public lastRequestTime;
    mapping(address => uint256) public hourlyTxCount;
    mapping(address => uint256) public lastTxHour;
    
    event Dispensed(address indexed requester, uint256 amount);
    event RateLimitsChanged(uint256 indexed newMaxTxPerHour, uint256 indexed newTimeLimit);
    event FundsRetrieved(address indexed owner, uint256 amount);

    /**
     * @dev Constructor to initialize rate limits
     * @param initialMaxTxPerHour Initial maximum transactions per hour
     * @param initialTimeLimit Initial time limit between requests
     */
    constructor(uint256 initialMaxTxPerHour, uint256 initialTimeLimit) {
        _validateRateLimits(initialMaxTxPerHour, initialTimeLimit);
        maxTxPerHour = initialMaxTxPerHour;
        timeLimit = initialTimeLimit;
    }

    /**
     * @dev Dispenses ETH to the requester if rate limits are not exceeded
     * @notice This function is protected against reentrancy and can be paused
     */
    function requestFunds() external nonReentrant whenNotPaused {
        // Check contract balance
        require(address(this).balance >= DISPENSE_AMOUNT, "Faucet: Insufficient contract balance");

        // Validate time-based limits
        require(block.timestamp - lastRequestTime[msg.sender] >= timeLimit, "Faucet: Request too soon");

        // Update hourly transaction count
        uint256 currentHour = block.timestamp / 1 hours;
        if (lastTxHour[msg.sender] != currentHour) {
            hourlyTxCount[msg.sender] = 0;
            lastTxHour[msg.sender] = currentHour;
        }
        
        require(hourlyTxCount[msg.sender] < maxTxPerHour, "Faucet: Max transactions per hour exceeded");

        // Update state before transfer
        lastRequestTime[msg.sender] = block.timestamp;
        hourlyTxCount[msg.sender] += 1;

        // Transfer funds
        (bool success, ) = payable(msg.sender).call{value: DISPENSE_AMOUNT}("");
        require(success, "Faucet: Transfer failed");

        emit Dispensed(msg.sender, DISPENSE_AMOUNT);
    }

    /**
     * @dev Allows the owner to retrieve ETH from the contract
     */
    function retrieveFunds() external onlyOwner {
        uint256 withdrawAmount = address(this).balance;
        require(withdrawAmount > 0, "Faucet: No balance to retrieve");

        (bool success, ) = payable(owner()).call{value: withdrawAmount}("");
        require(success, "Faucet: Transfer failed");
        
        emit FundsRetrieved(owner(), withdrawAmount);
    }

    /**
     * @dev Allows the owner to change the rate limits
     * @param newMaxTxPerHour The new maximum transactions per hour
     * @param newTimeLimit The new time limit between requests
     */
    function setRateLimits(uint256 newMaxTxPerHour, uint256 newTimeLimit) external onlyOwner {
        _validateRateLimits(newMaxTxPerHour, newTimeLimit);

        maxTxPerHour = newMaxTxPerHour;
        timeLimit = newTimeLimit;
        
        emit RateLimitsChanged(newMaxTxPerHour, newTimeLimit);
    }

    /**
     * @dev Internal function to validate rate limit parameters
     */
    function _validateRateLimits(uint256 newMaxTxPerHour, uint256 newTimeLimit) internal pure {
        require(newMaxTxPerHour > 0, "Faucet: Max transactions per hour must be greater than 0");
        require(newTimeLimit > 0, "Faucet: Time limit must be greater than 0");
    }

    /**
     * @dev Allows the owner to pause the contract
     */
    function pause() external onlyOwner {
        _pause();
    }

    /**
     * @dev Allows the owner to unpause the contract
     */
    function unpause() external onlyOwner {
        _unpause();
    }

    /**
     * @dev Fallback function to accept ETH deposits
     */
    receive() external payable {}
}