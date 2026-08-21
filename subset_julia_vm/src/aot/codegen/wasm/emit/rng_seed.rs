use sha2::{Digest, Sha256};
use wasm_encoder::{Function, Instruction as W, ValType};

pub(super) const SEED_NAME: &str = "__sjulia_rng_seed";

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const ROUND: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub(super) fn initial_state() -> [u64; 4] {
    expand(0)
}

fn expand(seed: u64) -> [u64; 4] {
    let hash = Sha256::digest(seed.to_le_bytes());
    std::array::from_fn(|index| {
        let start = index * 8;
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(&hash[start..start + 8]);
        u64::from_le_bytes(bytes)
    })
}

pub(super) fn emit_seed(state: [u32; 4]) -> Function {
    let mut body = Function::new([(26, ValType::I32)]);
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I32WrapI64);
    emit_bswap(&mut body);
    body.instruction(&W::LocalSet(1));
    body.instruction(&W::LocalGet(0));
    body.instruction(&W::I64Const(32));
    body.instruction(&W::I64ShrU);
    body.instruction(&W::I32WrapI64);
    emit_bswap(&mut body);
    body.instruction(&W::LocalSet(2));
    body.instruction(&W::I32Const(i32::MIN));
    body.instruction(&W::LocalSet(3));
    for local in 4..16 {
        body.instruction(&W::I32Const(0));
        body.instruction(&W::LocalSet(local));
    }
    body.instruction(&W::I32Const(64));
    body.instruction(&W::LocalSet(16));
    for (offset, value) in INITIAL.into_iter().enumerate() {
        body.instruction(&W::I32Const(value as i32));
        body.instruction(&W::LocalSet(17 + offset as u32));
    }
    for round in 0..64_u32 {
        let word = 1 + round % 16;
        if round >= 16 {
            emit_schedule(&mut body, word, round);
        }
        emit_round(&mut body, word, ROUND[round as usize]);
    }
    for pair in 0..4_u32 {
        emit_digest_word(&mut body, 17 + pair * 2, INITIAL[(pair * 2) as usize]);
        emit_bswap(&mut body);
        body.instruction(&W::I64ExtendI32U);
        emit_digest_word(&mut body, 18 + pair * 2, INITIAL[(pair * 2 + 1) as usize]);
        emit_bswap(&mut body);
        body.instruction(&W::I64ExtendI32U);
        body.instruction(&W::I64Const(32));
        body.instruction(&W::I64Shl);
        body.instruction(&W::I64Or);
        body.instruction(&W::GlobalSet(state[pair as usize]));
    }
    body.instruction(&W::End);
    body
}

fn emit_bswap(body: &mut Function) {
    body.instruction(&W::LocalSet(25));
    for (shift, mask, left) in [
        (24, -16_777_216_i32, true),
        (8, 16_711_680_i32, true),
        (8, 65_280_i32, false),
        (24, 255_i32, false),
    ] {
        body.instruction(&W::LocalGet(25));
        body.instruction(&W::I32Const(shift));
        body.instruction(if left { &W::I32Shl } else { &W::I32ShrU });
        body.instruction(&W::I32Const(mask));
        body.instruction(&W::I32And);
        if shift != 24 || !left {
            body.instruction(&W::I32Or);
        }
    }
}

fn emit_schedule(body: &mut Function, word: u32, round: u32) {
    emit_sigma(body, 1 + (round - 15) % 16, [7, 18, 3], false);
    body.instruction(&W::LocalGet(1 + (round - 16) % 16));
    body.instruction(&W::I32Add);
    emit_sigma(body, 1 + (round - 2) % 16, [17, 19, 10], false);
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalGet(1 + (round - 7) % 16));
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalSet(word));
}

fn emit_round(body: &mut Function, word: u32, constant: u32) {
    body.instruction(&W::LocalGet(24));
    emit_sigma(body, 21, [6, 11, 25], true);
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalGet(21));
    body.instruction(&W::LocalGet(22));
    body.instruction(&W::I32And);
    body.instruction(&W::LocalGet(21));
    body.instruction(&W::I32Const(-1));
    body.instruction(&W::I32Xor);
    body.instruction(&W::LocalGet(23));
    body.instruction(&W::I32And);
    body.instruction(&W::I32Xor);
    body.instruction(&W::I32Add);
    body.instruction(&W::I32Const(constant as i32));
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalGet(word));
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalSet(25));
    emit_sigma(body, 17, [2, 13, 22], true);
    body.instruction(&W::LocalGet(17));
    body.instruction(&W::LocalGet(18));
    body.instruction(&W::I32And);
    body.instruction(&W::LocalGet(17));
    body.instruction(&W::LocalGet(19));
    body.instruction(&W::I32And);
    body.instruction(&W::I32Xor);
    body.instruction(&W::LocalGet(18));
    body.instruction(&W::LocalGet(19));
    body.instruction(&W::I32And);
    body.instruction(&W::I32Xor);
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalSet(26));
    body.instruction(&W::LocalGet(23));
    body.instruction(&W::LocalSet(24));
    body.instruction(&W::LocalGet(22));
    body.instruction(&W::LocalSet(23));
    body.instruction(&W::LocalGet(21));
    body.instruction(&W::LocalSet(22));
    body.instruction(&W::LocalGet(20));
    body.instruction(&W::LocalGet(25));
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalSet(21));
    body.instruction(&W::LocalGet(19));
    body.instruction(&W::LocalSet(20));
    body.instruction(&W::LocalGet(18));
    body.instruction(&W::LocalSet(19));
    body.instruction(&W::LocalGet(17));
    body.instruction(&W::LocalSet(18));
    body.instruction(&W::LocalGet(25));
    body.instruction(&W::LocalGet(26));
    body.instruction(&W::I32Add);
    body.instruction(&W::LocalSet(17));
}

fn emit_sigma(body: &mut Function, local: u32, shifts: [i32; 3], rotate: bool) {
    for (index, shift) in shifts.into_iter().enumerate() {
        body.instruction(&W::LocalGet(local));
        body.instruction(&W::I32Const(shift));
        body.instruction(if rotate || index < 2 {
            &W::I32Rotr
        } else {
            &W::I32ShrU
        });
        if index > 0 {
            body.instruction(&W::I32Xor);
        }
    }
}

fn emit_digest_word(body: &mut Function, local: u32, initial: u32) {
    body.instruction(&W::LocalGet(local));
    body.instruction(&W::I32Const(initial as i32));
    body.instruction(&W::I32Add);
}
