/// File entry status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum EntryStatus {
    Deleted = 0,
    Present = 1,
}

impl EntryStatus {
    pub fn from_i16(v: i16) -> Option<Self> {
        match v {
            0 => Some(Self::Deleted),
            1 => Some(Self::Present),
            _ => None,
        }
    }

    pub fn as_i16(self) -> i16 {
        self as i16
    }
}
