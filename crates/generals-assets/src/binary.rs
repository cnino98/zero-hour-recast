use crate::AssetError;

pub struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    pub const fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    pub fn take(&mut self, length: usize) -> Result<&'a [u8], AssetError> {
        let (taken, rest) = self
            .remaining
            .split_at_checked(length)
            .ok_or(AssetError::Invalid("truncated input"))?;
        self.remaining = rest;
        Ok(taken)
    }

    pub fn byte(&mut self) -> Result<u8, AssetError> {
        Ok(self.take(1)?[0])
    }

    pub fn le_u16(&mut self) -> Result<u16, AssetError> {
        let data = self.take(2)?;
        Ok(u16::from_le_bytes([data[0], data[1]]))
    }

    pub fn le_u32(&mut self) -> Result<u32, AssetError> {
        let data = self.take(4)?;
        Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    pub fn be_u32(&mut self) -> Result<u32, AssetError> {
        let data = self.take(4)?;
        Ok(u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
    }

    pub fn nul_string(&mut self) -> Result<&'a [u8], AssetError> {
        let length = self
            .remaining
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(AssetError::Invalid("unterminated archive filename"))?;
        let name = self.take(length)?;
        self.take(1)?;
        Ok(name)
    }
}

pub fn usize_from_u32(value: u32) -> Result<usize, AssetError> {
    usize::try_from(value).map_err(|_| AssetError::Invalid("size cannot fit address space"))
}
