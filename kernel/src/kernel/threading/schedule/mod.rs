use conquer_once::spin::OnceCell;
use spin::Mutex;

use crate::{arch, serial_println};

use super::task::{Task, TaskID};

mod round_robin;

pub trait Scheduler {
    fn new() -> Self;
    fn add_task(&mut self, task: Task);
    fn yield_now(&mut self);
    fn cleanup(&mut self);
    fn kill(&mut self, id: TaskID);
    fn switch(&mut self, frame: &mut arch::interrupt::handlers::InterruptStackFrame);
    fn init(&mut self);
    fn current(&self) -> Option<&Task>;
    fn num_tasks(&self) -> usize;
    fn reschedule(&mut self, order: ScheduleOrder);
}

pub enum ScheduleOrder {}

type GlobalScheduler = round_robin::RoundRobin;

pub static GLOBAL_SCHEDULER: OnceCell<Mutex<GlobalScheduler>> = OnceCell::uninit();

pub fn init() {
    _ = GLOBAL_SCHEDULER.try_init_once(|| Mutex::new(GlobalScheduler::new()));
}

// TODO: make this a naked/asm function, which saves teh current context first thing and then calls scheduler.switch(), ...
pub fn switch(frame: &mut arch::interrupt::handlers::InterruptStackFrame) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn context_switch(
    state: *const arch::context::ReducedCpuInfo,
    frame: *const *const arch::interrupt::handlers::InterruptStackFrame,
) {
    serial_println!("state: {:#?}", *state);
    serial_println!("ptr2: {:#?}, ptr1: {:#?}", frame, *frame);
    serial_println!("frame: {:#?}", **frame);
}
