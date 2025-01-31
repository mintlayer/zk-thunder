import { expect } from "chai";
import { ethers } from "hardhat";
import { Contract, Signer } from "ethers";
import { time } from "@nomicfoundation/hardhat-network-helpers";

describe("Faucet", function () {
  let faucet: Contract;
  let owner: Signer;
  let requester: Signer;
  let ownerAddress: string;
  let requesterAddress: string;

  const INITIAL_MAX_TX_PER_HOUR = 10;
  const INITIAL_TIME_LIMIT = 24 * 60 * 60; // 24 hours

  beforeEach(async function () {
    [owner, requester] = await ethers.getSigners();
    ownerAddress = await owner.getAddress();
    requesterAddress = await requester.getAddress();

    const Faucet = await ethers.getContractFactory("Faucet");
    faucet = await Faucet.deploy(INITIAL_MAX_TX_PER_HOUR, INITIAL_TIME_LIMIT);
    await faucet.deployed();

    // Fund the faucet
    await owner.sendTransaction({
      to: faucet.address,
      value: ethers.utils.parseEther("1.0"),
    });
  });

  describe("Basic Functionality", function () {
    it("Should dispense 0.1 ETH to requester", async function () {
      const initialBalance = await ethers.provider.getBalance(requesterAddress);
      await faucet.connect(requester).requestFunds();
      const finalBalance = await ethers.provider.getBalance(requesterAddress);
      
      // Account for gas costs by checking the difference is close to 0.1 ETH
      expect(finalBalance.sub(initialBalance)).to.be.closeTo(
        ethers.utils.parseEther("0.1"),
        ethers.utils.parseEther("0.01") // Allow for gas costs
      );
    });

    it("Should not dispense funds more than once within time limit", async function () {
      await faucet.connect(requester).requestFunds();
      await expect(
        faucet.connect(requester).requestFunds()
      ).to.be.revertedWith("Faucet: Request too soon");
    });

    it("Should allow requests after time limit", async function () {
      await faucet.connect(requester).requestFunds();
      await time.increase(INITIAL_TIME_LIMIT);
      await expect(faucet.connect(requester).requestFunds()).to.not.be.reverted;
    });
  });

  describe("Rate Limiting", function () {
    it("Should enforce hourly transaction limits", async function () {
      await faucet.connect(owner).setRateLimits(2, INITIAL_TIME_LIMIT);

      await faucet.connect(requester).requestFunds();
      await faucet.connect(requester).requestFunds();
      await expect(
        faucet.connect(requester).requestFunds()
      ).to.be.revertedWith("Faucet: Max transactions per hour exceeded");

      await time.increase(3600); // Advance 1 hour
      await expect(faucet.connect(requester).requestFunds()).to.not.be.reverted;
    });

    it("Should not accept invalid rate limits", async function () {
      await expect(
        faucet.connect(owner).setRateLimits(0, INITIAL_TIME_LIMIT)
      ).to.be.revertedWith("Faucet: Max transactions per hour must be greater than 0");

      await expect(
        faucet.connect(owner).setRateLimits(INITIAL_MAX_TX_PER_HOUR, 0)
      ).to.be.revertedWith("Faucet: Time limit must be greater than 0");
    });
  });

  describe("Owner Functions", function () {
    it("Should allow owner to retrieve funds", async function () {
      const initialBalance = await ethers.provider.getBalance(ownerAddress);
      const tx = await faucet.connect(owner).retrieveFunds();
      const receipt = await tx.wait();
      const gasUsed = receipt.gasUsed.mul(receipt.effectiveGasPrice);
      
      const finalBalance = await ethers.provider.getBalance(ownerAddress);
      const expectedBalance = initialBalance.sub(gasUsed).add(ethers.utils.parseEther("1.0"));
      
      expect(finalBalance).to.equal(expectedBalance);
    });

    it("Should not allow non-owner to retrieve funds", async function () {
      await expect(
        faucet.connect(requester).retrieveFunds()
      ).to.be.revertedWith("Ownable: caller is not the owner");
    });

    it("Should allow owner to pause and unpause", async function () {
      await faucet.connect(owner).pause();
      await expect(
        faucet.connect(requester).requestFunds()
      ).to.be.revertedWith("Pausable: paused");

      await faucet.connect(owner).unpause();
      await expect(faucet.connect(requester).requestFunds()).to.not.be.reverted;
    });
  });

  describe("Events", function () {
    it("Should emit correct events", async function () {
      await expect(faucet.connect(requester).requestFunds())
        .to.emit(faucet, "Dispensed")
        .withArgs(requesterAddress, ethers.utils.parseEther("0.1"));

      await expect(faucet.connect(owner).setRateLimits(5, 12 * 60 * 60))
        .to.emit(faucet, "RateLimitsChanged")
        .withArgs(5, 12 * 60 * 60, ownerAddress);

      const balance = await ethers.provider.getBalance(faucet.address);
      await expect(faucet.connect(owner).retrieveFunds())
        .to.emit(faucet, "FundsRetrieved")
        .withArgs(ownerAddress, balance);
    });
  });

  describe("Edge Cases", function () {
    it("Should handle insufficient contract balance", async function () {
      await faucet.connect(owner).retrieveFunds(); // Empty the contract
      await expect(
        faucet.connect(requester).requestFunds()
      ).to.be.revertedWith("Faucet: Insufficient contract balance");
    });

    it("Should handle ownership transfer correctly", async function () {
      const [, newOwner] = await ethers.getSigners();
      const newOwnerAddress = await newOwner.getAddress();

      await faucet.connect(owner).transferOwnership(newOwnerAddress);
      await faucet.connect(newOwner).acceptOwnership();

      await expect(
        faucet.connect(owner).retrieveFunds()
      ).to.be.revertedWith("Ownable: caller is not the owner");

      await expect(
        faucet.connect(newOwner).retrieveFunds()
      ).to.not.be.reverted;
    });
  });
});