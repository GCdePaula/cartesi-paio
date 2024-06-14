// (c) Cartesi and individual authors (see AUTHORS)
// SPDX-License-Identifier: Apache-2.0 (see LICENSE)

pragma solidity ^0.8.18;


contract InputBox {

    event InputAdded(
        address indexed appContract,
        uint256 indexed index,
        bytes input
    );
    /// @notice Mapping of application contract addresses to arrays of input hashes.
    mapping(address => bytes32[]) private _inputBoxes;

    function evmAdvance(
        uint256 chainId,
        address appContract,
        address msgSender,
        uint256 blockNumber,
        uint256 blockTimestamp,
        uint256 prevRandao,
        uint256 index,
        bytes calldata payload
    ) external {}

    // inheritdoc IInputBox
    function addInput(
        address appContract,
        bytes calldata payload
    ) external returns (bytes32) {
        bytes32[] storage inputBox = _inputBoxes[appContract];
        uint256 index = inputBox.length;
        bytes memory input = abi.encodeCall(
            InputBox.evmAdvance,
            (
                block.chainid,
                appContract,
                msg.sender,
                block.number,
                block.timestamp,
                block.prevrandao,
                index,
                payload
            )
        );

        bytes32 inputHash = keccak256(input);

        inputBox.push(inputHash);

        emit InputAdded(appContract, index, input);

        return inputHash;
    }

    function getNumberOfInputs(
        address appContract
    ) external view returns (uint256) {
        return _inputBoxes[appContract].length;
    }

    function getInputHash(
        address appContract,
        uint256 index
    ) external view returns (bytes32) {
        return _inputBoxes[appContract][index];
    }
}