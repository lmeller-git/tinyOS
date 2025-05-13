use super::Scheduler;
use crate::{
    arch::{self, context::TaskCtx},
    kernel::threading::task::Task,
};
use alloc::{collections::vec_deque::VecDeque, vec::Vec};

pub struct RoundRobin {
    ready: VecDeque<Task>,
    blocking: Vec<Task>,
    running: Option<Task>,
}

impl Scheduler for RoundRobin {
    fn new() -> Self {
        Self {
            ready: VecDeque::new(),
            blocking: Vec::new(),
            running: None,
        }
    }

    fn add_task(&mut self, task: Task) {
        self.ready.push_back(task);
    }

    fn yield_now(&mut self) {
        todo!()
    }

    fn cleanup(&mut self) {
        todo!()
    }

    fn kill(&mut self, id: crate::kernel::threading::task::TaskID) {
        todo!()
    }

    fn switch(&mut self, frame: &mut arch::interrupt::handlers::InterruptStackFrame) {
        if let Some(mut current) = self.running.take() {
            // save context, push task to ready
            let ctx: &mut TaskCtx = &mut current.ctx;
            ctx.rsp = frame.stack_pointer.as_u64();
            ctx.rip = frame.instruction_pointer.as_u64();
            ctx.rflags = frame.cpu_flags.bits();
            ctx.cs = frame.code_segment.0 as u64;
            ctx.ss = frame.stack_segment.0 as u64;
            ctx.store_current();
            self.ready.push_back(current);
        }
        // load next task, context switch and return
        if let Some(mut next) = self.ready.pop_front() {}
        todo!()
    }

    fn init(&mut self) {
        todo!()
    }

    fn current(&self) -> Option<&Task> {
        todo!()
    }

    fn num_tasks(&self) -> usize {
        self.ready.len() + self.blocking.len() + if self.running.is_some() { 1 } else { 0 }
    }

    fn reschedule(&mut self, order: super::ScheduleOrder) {
        todo!()
    }
}
