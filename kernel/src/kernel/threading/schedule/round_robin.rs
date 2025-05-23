use super::{OneOneScheduler, Scheduler};
use crate::{
    arch::{self, context::TaskCtx},
    kernel::threading::task::{SimpleTask, Task},
    serial_println,
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

    fn switch(&mut self, ctx: TaskCtx) -> Option<&TaskCtx> {
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

pub struct OneOneRoundRobin {
    ready: VecDeque<SimpleTask>,
    blocking: Vec<SimpleTask>,
    running: Option<SimpleTask>,
}

impl OneOneScheduler for OneOneRoundRobin {
    fn new() -> Self {
        Self {
            ready: VecDeque::new(),
            blocking: Vec::new(),
            running: None,
        }
    }

    fn add_task(&mut self, task: SimpleTask) {
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

    fn switch(&mut self) -> Option<&SimpleTask> {
        if let Some(next) = self.ready.pop_front() {
            // serial_println!("ok");
            if let Some(current) = self.running.replace(next) {
                self.ready.push_back(current);
            }
        }
        // serial_println!("{:#?}", self.current());
        self.current()
    }

    fn init(&mut self) {
        todo!()
    }

    fn current(&self) -> Option<&SimpleTask> {
        self.running.as_ref()
    }

    fn num_tasks(&self) -> usize {
        self.ready.len() + self.blocking.len() + if self.running.is_some() { 1 } else { 0 }
    }

    fn reschedule(&mut self, order: super::ScheduleOrder) {
        todo!()
    }

    fn current_mut(&mut self) -> &mut Option<SimpleTask> {
        &mut self.running
    }
}
