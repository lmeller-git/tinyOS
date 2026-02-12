use crate::flags::OpenOptions;

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
    Pipe = 27,
    SpawnProcess = 28,
}

#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysErrCode {
    NoErr = 0,
    AccessDenied,
    OpDenied,
    AddrInUse,
    AddrNotAvail,
    AddrNotValid,
    BadFd,
    BadMsg,
    BadRqstD,
    Cancelled,
    NoChild,
    SendErr,
    Deadlock,
    DiskFull,
    FileExists,
    FileTooBig,
    InvalidArg,
    IO,
    NoDevice,
    NoFile,
    OOM,
    DirNotEmpty,
    InvalidSeek,
    NoProcess,
    TimerExp,
    WouldBlock,
}

const MAX_ERRNO: u64 = 25;

impl TryFrom<u64> for SysErrCode {
    type Error = i64;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value > MAX_ERRNO {
            Err(-1)
        } else {
            Ok(unsafe { core::mem::transmute(value) })
        }
    }
}

pub trait FromSyscall: Sized {
    fn try_parse_from(rax: u64, rdx: u64) -> Option<Self>;
    fn parse_from(rax: u64, rdx: u64) -> Self {
        Self::try_parse_from(rax, rdx).unwrap()
    }
}

pub type SysResult<T> = Result<T, SysErrCode>;

impl<T: TryFrom<u64>> FromSyscall for SysResult<T> {
    fn try_parse_from(rax: u64, rdx: u64) -> Option<Self> {
        let errno = rdx.try_into().ok()?;
        if errno == SysErrCode::NoErr {
            Some(Ok(rax.try_into().ok()?))
        } else {
            Some(Err(errno))
        }
    }
}

pub type SysCallRes<T> = SysResult<T>;

pub type FileDescriptor = u32;

#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct FatPtr<T> {
    pub size: usize,
    pub thin: *const T,
}

#[derive(Debug)]
#[repr(C)]
pub enum FDAction {
    Open(FDOpen, FileDescriptor),
    Close(FileDescriptor),
    Dup(FileDescriptor, FileDescriptor),
    Clear,
}

#[repr(C)]
#[derive(Debug)]
pub struct FDOpen {
    pub path: FatPtr<u8>,
    pub flags: OpenOptions,
}
