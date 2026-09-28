//! MSB-first bit reader over one buffer. Every read is bounds-checked and
//! running out of data is an [`Error::Invalid`], never a panic.

use crate::error::{Error, Result, invalid};

pub(crate) struct BitReader<'a> {
    data: &'a [u8],
    /// Bits consumed from the start of `data`.
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Bits consumed.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Bits left.
    pub fn remaining(&self) -> usize {
        self.data.len() * 8 - self.pos
    }

    fn short() -> Error {
        invalid("the data ends inside a syntax element")
    }

    /// Read `n <= 32` bits as an unsigned integer.
    pub fn read(&mut self, n: u32) -> Result<u32> {
        debug_assert!(n <= 32);
        if n == 0 {
            return Ok(0);
        }
        if self.remaining() < n as usize {
            return Err(Self::short());
        }
        let v = self.peek_unchecked(n);
        self.pos += n as usize;
        Ok(v)
    }

    pub fn bit(&mut self) -> Result<bool> {
        let byte = *self.data.get(self.pos / 8).ok_or_else(Self::short)?;
        let b = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Ok(b == 1)
    }

    /// The next `n <= 32` bits without consuming them; bits past the end
    /// read as zero.
    pub fn peek(&self, n: u32) -> u32 {
        self.peek_unchecked(n)
    }

    fn peek_unchecked(&self, n: u32) -> u32 {
        let byte = self.pos / 8;
        let mut acc: u64 = 0;
        for i in 0..5 {
            acc = (acc << 8) | u64::from(*self.data.get(byte + i).unwrap_or(&0));
        }
        let shift = 40 - (self.pos % 8) as u32 - n;
        ((acc >> shift) & ((1u64 << n) - 1)) as u32
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        if self.remaining() < n {
            return Err(Self::short());
        }
        self.pos += n;
        Ok(())
    }

    /// Advance to the next byte boundary of the buffer.
    pub fn align(&mut self) {
        self.pos = self.pos.div_ceil(8) * 8;
        self.pos = self.pos.min(self.data.len() * 8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_msb_first_and_stops_at_the_end() {
        let mut r = BitReader::new(&[0b1010_0101, 0xff, 0x01]);
        assert!(r.bit().unwrap());
        assert_eq!(r.read(3).unwrap(), 0b010);
        assert_eq!(r.peek(8), 0b0101_1111);
        assert_eq!(r.read(12).unwrap(), 0b0101_1111_1111);
        assert_eq!(r.remaining(), 8);
        r.align();
        assert_eq!(r.read(8).unwrap(), 1);
        assert!(r.read(1).is_err());
        assert!(r.bit().is_err());
        assert_eq!(r.peek(16), 0);
        let mut r = BitReader::new(&[0xde, 0xad, 0xbe, 0xef, 0x12]);
        r.skip(4).unwrap();
        assert_eq!(r.read(32).unwrap(), 0xeadb_eef1);
    }
}
