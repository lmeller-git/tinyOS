use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    sync::Arc,
    vec::Vec,
};
use core::{
    fmt::Debug,
    sync::atomic::{AtomicU64, Ordering},
};

use conquer_once::spin::OnceCell;
use hashbrown::HashMap;

use crate::{
    kernel::{
        fd::MaybeOwned,
        threading::{
            schedule::{GlobalTaskPtr, Scheduler},
            task::{
                ExitInfo,
                ProcessGroupID,
                ProcessID,
                TaskCore,
                TaskRepr,
                TaskState,
                TaskStateData,
                ThreadID,
            },
        },
    },
    sync::locks::{Mutex, RwLock},
};

static GLOBAL_TASK_MANAGER: OnceCell<TaskManager> = OnceCell::uninit();

// TODO:
// we want process groupes -> process -> threads
// a single session is assumed, thus we do not provide session api
// one process group is foreground and connected to the controlling tty
// the ID of a thread is ProcessGroupID - ProcessID - ThreadID
// all threads are cleaned up once a process exits
// all members of a process group are sent HUP + CONT if it becomes orphaned (ie it leader exits)
// TODO this also requires an implementation of signals + signal hooks
// TODO we also need some form of a controlling tty
// --> could store in a HashMap of Hash(PGID, PID, TID), with current being the hash
// Either:
// store only Hash + (intrusive?) tree of processes and walk on query
// or:
// store Hashmap and build process data on the fly if queried / store in additional tree

// TODO may want to add ids to Process, ProcessGroup

///
/// Usage:
/// ```
/// let state = tls::task_data().unwrap();
/// let current = state.current_thread().unwrap();
/// let current_process = state.current_pgr().unwrap().current_process().unwrap();
///
/// // Kill Thread
/// let id = 0;
/// _ = state.kill(&id.into());
///
/// // Kill Process
///
/// ```

#[derive(Default, Debug)]
pub struct ProcessGroup {
    members: BTreeMap<ProcessID, Arc<RwLock<Process>>>,
    leader: Option<ProcessID>,
}

impl ProcessGroup {
    pub fn new(id: ProcessID, leader: Process) -> Self {
        let mut members = BTreeMap::new();
        members.insert(id, RwLock::new(leader).into());
        Self {
            members,
            leader: Some(id),
        }
    }

    pub fn add(&mut self, id: ProcessID, process: Process) -> Option<Arc<RwLock<Process>>> {
        self.members.insert(id, RwLock::new(process).into())
    }

    pub fn next_pid(&self) -> ProcessID {
        static CURRENT_PID: AtomicU64 = AtomicU64::new(0);
        let current = CURRENT_PID.fetch_add(1, Ordering::AcqRel);
        ProcessID(current)
    }

    pub fn current_process(&self) -> Option<Arc<RwLock<Process>>> {
        self.members
            .get(&task_data().current_thread()?.pid())
            .cloned()
    }
}

#[derive(Default, Debug)]
pub struct Process {
    threads: BTreeMap<ThreadID, GlobalTaskPtr>,
}

impl Process {
    pub fn new(leader: GlobalTaskPtr) -> Self {
        let mut threads = BTreeMap::new();
        threads.insert(leader.tid(), leader);
        Self { threads }
    }
}

#[derive(Debug)]
pub struct TaskManager {
    current_running: AtomicU64, // TaskID of the curently active thread
    lut: RwLock<HashMap<ThreadID, GlobalTaskPtr>>, // LUT for thread id --> thread
    processes: RwLock<HashMap<ProcessID, MaybeOwned<TaskCore>>>, // MaybeOwned here is never owned, as each core is shared with at least one thread. This is enforced by TaskBuilder
    tree: RwLock<BTreeMap<ProcessGroupID, Arc<RwLock<ProcessGroup>>>>,
    zombies: Mutex<VecDeque<ThreadID>>,
}

impl TaskManager {
    fn new() -> Self {
        Self {
            current_running: ThreadID::default().get_inner().into(),
            lut: RwLock::default(),
            processes: RwLock::default(),
            tree: RwLock::default(),
            zombies: Mutex::default(),
        }
    }

    pub fn thread(&self, task: &ThreadID) -> Option<GlobalTaskPtr> {
        self.lut.read().get(task).cloned()
    }

    pub fn current_thread(&self) -> Option<GlobalTaskPtr> {
        self.thread(&self.current_tid())
    }

    pub fn try_thread(&self, task: &ThreadID) -> Option<GlobalTaskPtr> {
        self.lut.try_read()?.get(task).cloned()
    }

    pub fn try_current_thread(&self) -> Option<GlobalTaskPtr> {
        self.try_thread(&self.current_tid())
    }

    pub fn current_pgr(&self) -> Option<Arc<RwLock<ProcessGroup>>> {
        self.tree
            .read()
            .get(&self.current_thread()?.pgrid())
            .cloned()
    }

    pub fn current_tid(&self) -> ThreadID {
        (&self.current_running).into()
    }

    pub fn update_current(&self, task: ThreadID) {
        self.current_running
            .store(task.get_inner(), Ordering::Release);
    }

    /// thread
    pub fn add(&self, task: GlobalTaskPtr) -> Option<GlobalTaskPtr> {
        let pid = task.pid();
        _ = self.processes.write().insert(pid, task.core.try_clone()?);
        _ = self
            .tree
            .write()
            .entry(task.pgrid())
            .and_modify(|entry| {
                _ = entry.write().add(pid, Process::new(task.clone()));
            })
            .or_insert(RwLock::new(ProcessGroup::new(pid, Process::new(task.clone()))).into());

        self.lut.write().insert(task.tid(), task)
    }

    pub fn cleanup(&self) {
        let tasks = self.lut.read();
        for (id, task) in tasks.iter() {
            if task.state() == TaskState::Zombie {
                self.zombies.lock().push_back(*id);
            }
        }

        drop(tasks);
        // cleanup zombies and remove them from self.tasks
        while let Some(zombie) = self.zombies.lock().pop_front() {
            let Some(task) = self.lut.write().remove(&zombie) else {
                continue;
            };
            cleanup_task(task);
        }
    }

    /// thread
    pub fn update(&self, task: &GlobalTaskPtr) {
        match task.state() {
            TaskState::Zombie => {
                self.zombies.lock().push_back(task.tid());
            }
            _ => _ = self.add(task.clone()),
        }
    }

    pub fn get_table(&self) -> &RwLock<HashMap<ThreadID, GlobalTaskPtr>> {
        &self.lut
    }

    /// thread
    pub fn kill(&self, id: &ThreadID, signal: i32) -> Option<()> {
        let task = self.thread(id)?;
        task.set_state(TaskState::Zombie);
        *task.state_data().lock() = TaskStateData::Exit(ExitInfo {
            exit_code: signal as u32,
            signal: None,
        });
        self.update(&task);
        Some(())
    }

    /// thread
    pub fn block(&self, id: &ThreadID) -> Option<()> {
        let task = self.thread(id)?;
        if task.state() != TaskState::Zombie && task.state() != TaskState::Sleeping {
            task.set_state(TaskState::Blocking);
            Some(())
        } else {
            None
        }
    }

    /// thread
    pub fn try_block(&self, id: &ThreadID) -> Option<()> {
        let task = self.try_thread(id)?;
        if task.state() != TaskState::Zombie && task.state() != TaskState::Sleeping {
            task.set_state(TaskState::Blocking);
            Some(())
        } else {
            None
        }
    }

    /// thread
    pub fn try_wake(&self, id: &ThreadID) -> Option<()> {
        let task = self.try_thread(id)?;
        if task.state() == TaskState::Blocking || task.state() == TaskState::Sleeping {
            task.set_state(TaskState::Ready);
        }
        Some(())
    }

    /// thread
    pub fn wake(&self, id: &ThreadID) -> Option<()> {
        let task = self.thread(id)?;
        if task.state() == TaskState::Blocking || task.state() == TaskState::Sleeping {
            task.set_state(TaskState::Ready);
        }
        Some(())
    }

    /// process
    pub fn kill_process(&self, pid: &ProcessID) -> Option<()> {
        // this sucks.
        // might want to flatten th tree into maps of ids
        let group_id = self.processes.read().get(pid)?.pgrid;
        let tree = self.tree.read();
        let group = tree.get(&group_id)?.read();
        let process = group.members.get(pid)?.read();
        for id in process.threads.iter().map(|(id, _)| id) {
            self.kill(id, 0)?;
        }
        Some(())
    }

    pub fn next_pgrid(&self) -> ProcessGroupID {
        static CURRENT_PGRID: AtomicU64 = AtomicU64::new(0);
        let current = CURRENT_PGRID.fetch_add(1, Ordering::AcqRel);
        ProcessGroupID(current)
    }
}

pub fn task_data<'a>() -> &'a TaskManager {
    GLOBAL_TASK_MANAGER.get_or_init(TaskManager::new)
}

fn cleanup_task(task: GlobalTaskPtr) {
    // TODO
}
