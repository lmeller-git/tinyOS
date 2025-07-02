use core::{convert::Infallible, str::FromStr};

use super::{TestCase, TestConfig};

#[repr(C)]
pub struct RawStr {
    start: *const u8,
    len: usize,
}

impl RawStr {
    pub fn to_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.start, self.len)) }
    }

    pub const fn from_s_str(s: &'static str) -> Self {
        Self {
            start: s.as_ptr(),
            len: s.len(),
        }
    }
}

// unsafe. s might get dropped
impl FromStr for RawStr {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            start: s.as_ptr(),
            len: s.len(),
        })
    }
}

unsafe impl Sync for RawStr {}
unsafe impl Send for RawStr {}
