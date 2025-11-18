#[repr(u64)]
pub enum SysCallDispatch {
    Open = 0,
    Close = 1,
    Read = 2,
    Write = 3,
    Yield = 4,
    Exit = 5,
    Kill = 6,
    Mmap = 7,
    Munmap = 8,
    Fork = 9,
    WaitTime = 10,
    Machine = 11,
    GetPID = 12,
    Seek = 13,
    Dup = 14,
    Spawn = 15,
    Dbg = 16,
    Execve = 17,
    ThreadCreate = 18,
    ThreadExit = 19,
    ThreadCancel = 20,
    ThreadJoin = 21,
    WaitPID = 22,
    EventFD = 23,
    Time = 24,
    GetTID = 25,
    GetPgrID = 26,
}

#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysRetCode {
    Unknown = -2,
    Success = 0,
    Fail = -1,
}

impl TryFrom<i64> for SysRetCode {
    type Error = i64;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Success),
            -1 => Ok(Self::Fail),
            -2 => Ok(Self::Unknown),
            _ => Err(value),
        }
    }
}

pub type SysResult<T> = Result<T, SysRetCode>;
pub type SysCallRes<T> = SysResult<T>;

pub type FileDescriptor = u32;
