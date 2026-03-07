const { poseidonHashMany, hash } = require("starknet");

try {
    const val = hash.computePoseidonHashOnElements([1n, 1n]);
    console.log("computePoseidonHashOnElements:", val);
} catch(e) { console.log("error 1:", e.message); }

try {
    const val2 = poseidonHashMany([1n, 1n]);
    console.log("poseidonHashMany:", val2);
} catch(e) { console.log("error 2:", e.message); }
