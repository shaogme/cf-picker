use core::hash::{BuildHasher, Hasher};

use crate::traits::Rand;

const K: u64 = 0x9e3779b97f4a7c15;

#[inline]
fn fast_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = bytes.len() as u64;
    for chunk in bytes.chunks(8) {
        let mut val = 0u64;
        for (i, &b) in chunk.iter().enumerate() {
            val |= (b as u64) << (i * 8);
        }
        hash = hash.wrapping_add(val).wrapping_mul(K);
    }
    hash
}

/// 极速非安全哈希器，适用于完全受信任的内部数据
#[derive(Debug, Clone)]
pub struct FastHasher {
    hash: u64,
}

impl FastHasher {
    /// 使用指定的种子创建一个新的 FastHasher
    #[inline]
    pub const fn new(seed: u64) -> Self {
        Self { hash: seed }
    }
}

impl Default for FastHasher {
    #[inline]
    fn default() -> Self {
        Self::new(0)
    }
}

impl FastHasher {
    #[inline]
    fn add_to_hash(&mut self, i: u64) {
        self.hash = self.hash.wrapping_add(i).wrapping_mul(K);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        const ROTATE: u32 = 26;
        self.hash.rotate_left(ROTATE)
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.write_u64(fast_hash_bytes(bytes));
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add_to_hash(i as u64);
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add_to_hash(i as u64);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add_to_hash(i as u64);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add_to_hash(i);
    }

    #[inline]
    fn write_u128(&mut self, i: u128) {
        self.add_to_hash(i as u64);
        self.add_to_hash((i >> 64) as u64);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add_to_hash(i as u64);
    }
}

/// 支持在 HashMap / HashSet 中直接使用的极速非安全构造器
#[derive(Debug, Clone, Copy, Default)]
pub struct FastBuildHasher {
    seed: u64,
}

impl FastBuildHasher {
    /// 创建一个新的 FastBuildHasher
    #[inline]
    pub const fn new() -> Self {
        Self { seed: 0 }
    }

    #[inline]
    pub const fn with_seed(seed: u64) -> Self {
        Self { seed }
    }

    #[inline]
    pub fn from_rng<G: Rand + ?Sized>(rng: &mut G) -> Self {
        Self {
            seed: rng.next_u64(),
        }
    }
}

impl BuildHasher for FastBuildHasher {
    type Hasher = FastHasher;

    #[inline]
    fn build_hasher(&self) -> Self::Hasher {
        FastHasher::new(self.seed)
    }
}
