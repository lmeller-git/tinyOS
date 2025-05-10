use super::Scheduler;
use crate::kernel::threading::task::Task;
use alloc::collections::vec_deque::VecDeque;

pub struct RoundRobin {
    tasks: VecDeque<Task>,
}

impl Scheduler for RoundRobin {
    fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
        }
    }

    fn add_task(&mut self, task: Task) {
        self.tasks.push_back(task);
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

    fn switch(&mut self) {
        todo!()
    }

    fn init(&mut self) {
        todo!()
    }

    fn current(&self) -> Option<&Task> {
        todo!()
    }

    fn num_tasks(&self) -> usize {
        self.tasks.len()
    }

    fn reschedule(&mut self, order: super::ScheduleOrder) {
        todo!()
    }
}
