use super::{GlobalTaskPtr, OneOneScheduler};
use crate::{
    arch::context::TaskCtx,
    kernel::threading::{
        self,
        task::{SimpleTask, TaskID, TaskState},
    },
    serial_println,
};
use alloc::{collections::vec_deque::VecDeque, vec::Vec};
use hashbrown::HashMap;

pub struct OneOneRoundRobin {
    ready: VecDeque<GlobalTaskPtr>,
    lookup: HashMap<TaskID, GlobalTaskPtr>,
    running: Option<GlobalTaskPtr>,
}

impl OneOneScheduler for OneOneRoundRobin {
    fn new() -> Self {
        Self {
            ready: VecDeque::new(),
            lookup: HashMap::new(),
            running: None,
        }
    }

    fn add_task(&mut self, task: GlobalTaskPtr) {
        self.lookup
            .insert(task.with_inner(|inner| inner.pid.clone()), task.clone());
        self.ready.push_back(task);
    }

    fn yield_now(&mut self) {
        threading::yield_now();
    }

    fn cleanup(&mut self) {
        todo!()
    }

    fn kill(&mut self, id: crate::kernel::threading::task::TaskID) {
        todo!()
    }

    fn switch(&mut self) -> Option<GlobalTaskPtr> {
        while let Some(next) = self.ready.pop_front() {
            if next.read_inner().state != TaskState::Ready {
                // TODO do something with these tasks, instead of just ignoring
                // for now: they simply get popped and later added again via wake() (potentially). They continue to be stored in lookup, unless cleaned up
                continue;
            }
            if let Some(current) = self.running.replace(next) {
                // it is fine to push all tasks, as non rerady tasks will be popped in the (next) switch
                self.ready.push_back(current);
            }
            return self.current();
        }
        None
    }

    fn init(&mut self) {
        todo!()
    }

    fn current(&self) -> Option<GlobalTaskPtr> {
        self.running.clone()
    }

    fn num_tasks(&self) -> usize {
        // this potentially includes zombie tasks, ...
        self.lookup.len()
    }

    fn reschedule(&mut self, order: super::ScheduleOrder) {
        // removes all non ready tasks from queue and adds back ready tasks
        todo!()
    }

    fn current_mut(&mut self) -> &mut Option<GlobalTaskPtr> {
        &mut self.running
    }

    fn wake(&mut self, id: &TaskID) {
        if let Some(task) = self.lookup.get(id)
            && !self
                .ready
                .iter()
                .any(|task| task.with_inner(|inner| &inner.pid == id))
        {
            self.ready.push_back(task.clone());
        }
    }
}
