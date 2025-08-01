use core::sync::atomic::{AtomicU64, Ordering};

use alloc::collections::{btree_map::BTreeMap, vec_deque::VecDeque};
use conquer_once::spin::OnceCell;

use crate::{
    kernel::threading::{
        schedule::GlobalTaskPtr,
        task::{TaskID, TaskState},
    },
    sync::locks::{Mutex, RwLock},
};

static GLOBAL_TASK_MANAGER: OnceCell<TaskManager> = OnceCell::uninit();

pub struct TaskManager {
    tasks: RwLock<BTreeMap<TaskID, GlobalTaskPtr>>,
    current_running: AtomicU64, // TaskID
    zombies: Mutex<VecDeque<TaskID>>,
}

impl TaskManager {
    fn new() -> Self {
        Self {
            tasks: RwLock::new(BTreeMap::new()),
            current_running: TaskID::default().get_inner().into(),
            zombies: Mutex::new(VecDeque::new()),
        }
    }

    pub fn get(&self, task: &TaskID) -> Option<GlobalTaskPtr> {
        self.tasks.read().get(task).cloned()
    }

    pub fn get_current(&self) -> Option<GlobalTaskPtr> {
        self.get(&self.current_pid())
    }

    pub fn current_pid(&self) -> TaskID {
        (&self.current_running).into()
    }

    pub fn update_current(&self, task: TaskID) {
        self.current_running
            .store(task.get_inner(), Ordering::Release);
    }

    pub fn add(&self, task: GlobalTaskPtr) -> Option<GlobalTaskPtr> {
        let pid = task.read().pid;
        self.tasks.write().insert(pid, task)
    }

    pub fn cleanup(&self) {
        todo!()
    }

    pub fn update(&self, task: &GlobalTaskPtr) {
        let reader = task.read();
        match reader.state {
            TaskState::Zombie(_) => {
                self.zombies.lock().push_back(reader.pid);
            }
            _ => _ = self.add(task.clone()),
        }
    }

    pub fn get_table(&self) -> &RwLock<BTreeMap<TaskID, GlobalTaskPtr>> {
        &self.tasks
    }
}

pub fn task_data<'a>() -> &'a TaskManager {
    GLOBAL_TASK_MANAGER.get_or_init(|| TaskManager::new())
}
