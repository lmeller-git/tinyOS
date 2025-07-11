use funcs::{sys_exit, sys_kill, sys_yield};

use crate::arch::context::SysCallCtx;

mod funcs;

pub extern "C" fn syscall_handler(args: &mut SysCallCtx) {
    let res: SysRetCode = match args.num() {
        60 => {
            sys_exit(args.first() as i64);
            unreachable!()
        }
        62 => sys_kill(args.first(), args.second() as i64),
        451 => sys_yield(),
        _ => SysRetCode::Unknown,
    };

    args.ret(res as i64);
}

#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysRetCode {
    Unknown = -2,
    Success = 0,
    Fail = -1,
}
