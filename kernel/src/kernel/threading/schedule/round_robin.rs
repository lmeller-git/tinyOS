use super::{GlobalTaskPtr, OneOneScheduler};
use crate::{
    arch::context::TaskCtx,
    kernel::threading::{
        self,
        schedule::Scheduler,
        task::{SimpleTask, TaskID, TaskState},
        tls,
    },
    serial_println,
    sync::{self, locks::RwLock},
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
        self.lookup.insert(task.read().pid.clone(), task.clone());
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
            if next.read().state != TaskState::Ready {
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
            && !self.ready.iter().any(|task| task.read().pid == *id)
        {
            self.ready.push_back(task.clone());
        }
    }
}

pub struct LazyRoundRobin {
    queue: sync::locks::Mutex<VecDeque<TaskID>>,
}

impl Scheduler for LazyRoundRobin {
    fn new() -> Self {
        Self {
            queue: sync::locks::Mutex::new(VecDeque::new()),
        }
    }

    fn reschedule(&self) {
        // TODO return if not dirty
        let manager = tls::task_data();

        let table = manager.get_table();

        let mut queue = self.queue.lock();
        queue.clear();

        for (id, task) in table.read().iter() {
            if task.read().state == TaskState::Ready {
                queue.push_back(*id);
            }
        }
    }

    fn switch(&self) -> TaskID {
        let mut queue = self.queue.lock();
        while let Some(id) = queue.pop_front() {
            let Some(data) = tls::task_data().get(&id) else {
                // Task was likely killed and removed from task manager
                continue;
            };
            if data.read().state != TaskState::Ready {
                tls::task_data().update(&data);
                continue;
            }
            tls::task_data().update_current(id);
            queue.push_back(id);
            return id;
        }
        TaskID::default()
    }

    fn add_task(&self, id: TaskID) {
        self.queue.lock().push_back(id);
    }
}
