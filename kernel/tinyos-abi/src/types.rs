use crate::flags::{NodePermissions, NodeType, OpenOptions};

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
    FStat = 29,
    SetPerm = 30,
}

#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysErrCode {
    NoErr = 0,
    AccessDenied = 1,
    OpDenied = 2,
    AddrInUse = 3,
    AddrNotAvail = 5,
    AddrNotValid = 6,
    BadFd = 7,
    BadMsg = 8,
    BadRqstD = 9,
    Cancelled = 10,
    NoChild = 11,
    SendErr = 12,
    Deadlock = 13,
    DiskFull = 14,
    FileExists = 15,
    FileTooBig = 16,
    InvalidArg = 17,
    IO = 18,
    NoDevice = 19,
    NoFile = 20,
    OOM = 21,
    DirNotEmpty = 22,
    InvalidSeek = 23,
    NoProcess = 24,
    TimerExp = 25,
    WouldBlock = 26,
}

const MAX_ERRNO: u64 = 26;

impl TryFrom<u64> for SysErrCode {
    type Error = i64;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value > MAX_ERRNO {
            Err(-1)
        } else {
            Ok(unsafe { core::mem::transmute::<u64, Self>(value) })
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

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct FatPtr<T> {
    pub size: usize,
    pub thin: *const T,
}

#[repr(C)]
#[derive(Debug)]
pub enum FDAction {
    Open(FDOpen, FileDescriptor),
    Close(FileDescriptor),
    Dup(FileDescriptor, FileDescriptor),
    Inherit(FileDescriptor, FileDescriptor),
    Clear,
}

#[repr(C)]
#[derive(Debug)]
pub struct FDOpen {
    pub path: FatPtr<u8>,
    pub flags: OpenOptions,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FStat {
    /// time of creation in secs since startup
    pub t_create: u64,
    /// time of last modifcation in secs since startup
    pub t_mod: u64,
    pub size: usize,
    pub permissions: NodePermissions,
    pub node_type: NodeType,
}

impl Default for FStat {
    fn default() -> Self {
        Self {
            t_create: 0,
            t_mod: 0,
            size: usize::MAX,
            permissions: NodePermissions::default(),
            node_type: NodeType::VOID,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermUpdateStrategy {
    AND = 0,
    OR = 1,
    #[default]
    OVERWRITE = 2,
}

impl TryFrom<u64> for PermUpdateStrategy {
    type Error = u64;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::AND,
            1 => Self::OR,
            2 => Self::OVERWRITE,
            _ => Err(value)?,
        })
    }
}
