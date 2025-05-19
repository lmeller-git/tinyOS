use core::arch::{asm, global_asm};

use conquer_once::spin::OnceCell;
use spin::Mutex;

use crate::{
    arch::{
        self,
        context::{TaskCtx, set_cpu_context, switch_and_apply},
    },
    serial_println,
};

use super::task::{SimpleTask, Task, TaskID};

mod round_robin;

pub trait Scheduler {
    fn new() -> Self;
    fn add_task(&mut self, task: Task);
    fn yield_now(&mut self);
    fn cleanup(&mut self);
    fn kill(&mut self, id: TaskID);
    fn switch(&mut self, ctx: TaskCtx) -> Option<&TaskCtx>;
    fn init(&mut self);
    fn current(&self) -> Option<&Task>;
    fn num_tasks(&self) -> usize;
    fn reschedule(&mut self, order: ScheduleOrder);
}

pub trait OneOneScheduler {
    fn new() -> Self;
    fn add_task(&mut self, task: SimpleTask);
    fn yield_now(&mut self);
    fn cleanup(&mut self);
    fn kill(&mut self, id: TaskID);
    fn switch(&mut self) -> Option<&SimpleTask>;
    fn init(&mut self);
    fn current(&self) -> Option<&SimpleTask>;
    fn num_tasks(&self) -> usize;
    fn reschedule(&mut self, order: ScheduleOrder);
}

pub enum ScheduleOrder {}

type GlobalScheduler = round_robin::OneOneRoundRobin;

pub static GLOBAL_SCHEDULER: OnceCell<Mutex<GlobalScheduler>> = OnceCell::uninit();

pub fn init() {
    _ = GLOBAL_SCHEDULER.try_init_once(|| Mutex::new(GlobalScheduler::new()));
}

#[allow(unsafe_op_in_unsafe_fn)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch_local() {
    if let Some(new) = GLOBAL_SCHEDULER.get_unchecked().lock().switch() {
        switch_and_apply(new);
        unreachable!()
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch(
    state: arch::context::ReducedCpuInfo,
    frame: arch::interrupt::handlers::InterruptStackFrame,
) {
    // let ctx = TaskCtx::from_trap_ctx(frame, state);
    // if let Some(new) = GLOBAL_SCHEDULER.get_unchecked().lock().switch(ctx) {}
    // set_cpu_context(ctx);
}
