use alloc::collections::{binary_heap::BinaryHeap, vec_deque::VecDeque};
use core::{cmp::Reverse, fmt::Debug};

use conquer_once::spin::OnceCell;

use crate::{
    arch::x86::current_time,
    eprintln,
    kernel::threading::{task::ThreadID, tls, wait::condition::WaitCondition},
    serial_println,
    sync::locks::Mutex,
};

pub static TIMERQUEUE: OnceCell<TimeWaitQueue> = OnceCell::uninit();
pub static KEYBOARDQUEUE: OnceCell<KeyBoardQueue> = OnceCell::uninit();

pub(crate) trait WaitQueue {
    fn enqueue(&self, id: &ThreadID, condition: WaitCondition) -> Option<()>;
    fn signal(&self);
}

pub struct WaitNode {
    id: ThreadID,
    cond: WaitCondition,
}

impl WaitNode {
    pub fn new(id: ThreadID, cond: WaitCondition) -> Self {
        Self { id, cond }
    }
}

impl PartialEq for WaitNode {
    fn eq(&self, other: &Self) -> bool {
        self.cond.eq(&other.cond) && self.id.eq(&other.id)
    }
}

impl Eq for WaitNode {}

impl PartialOrd for WaitNode {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.cond
            .partial_cmp(&other.cond)
            .map(|c| c.then(self.id.cmp(&other.id)))
    }
}

impl Ord for WaitNode {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.cond.cmp(&other.cond).then(self.id.cmp(&other.id))
    }
}

impl Debug for WaitNode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitNode")
            .field("id", &self.id)
            .field("condition", &self.cond)
            .finish()
    }
}

pub struct TimeWaitQueue {
    inner: Mutex<BinaryHeap<Reverse<WaitNode>>>,
}

impl TimeWaitQueue {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(BinaryHeap::new()),
        }
    }
}

impl WaitQueue for TimeWaitQueue {
    fn enqueue(&self, id: &ThreadID, condition: WaitCondition) -> Option<()> {
        // TODO also allow None conoditions
        let WaitCondition::Time(_) = condition else {
            return None;
        };
        self.inner
            .lock()
            .push(Reverse(WaitNode::new(*id, condition)));
        Some(())
    }

    fn signal(&self) {
        let mut q = self.inner.lock();
        while let Some(w) = q.peek()
            && let Reverse(WaitNode {
                id,
                cond: WaitCondition::Time(t),
            }) = w
            && *t <= current_time()
        {
            if tls::task_data().wake(id).is_none() {
                eprintln!("could not wake up task with id {}", id);
            }
            q.pop();
        }
    }
}

impl Default for TimeWaitQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub struct KeyBoardQueue {
    q: Mutex<VecDeque<WaitNode>>,
}

impl KeyBoardQueue {
    pub fn new() -> Self {
        Self {
            q: Mutex::new(VecDeque::new()),
        }
    }
}

impl WaitQueue for KeyBoardQueue {
    fn enqueue(&self, id: &ThreadID, condition: WaitCondition) -> Option<()> {
        let node = WaitNode::new(*id, condition);
        self.q.lock().push_back(node);
        Some(())
    }

    fn signal(&self) {
        for node in self.q.lock().drain(..) {
            if tls::task_data().wake(&node.id).is_none() {
                eprintln!("could not wake up task with id {}", node.id);
            }
        }
    }
}

impl Default for KeyBoardQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GenericWaitQueue {
    q: Mutex<VecDeque<WaitNode>>,
}

impl GenericWaitQueue {
    pub fn new() -> Self {
        Self::default()
    }
}

impl WaitQueue for GenericWaitQueue {
    fn enqueue(&self, id: &ThreadID, condition: WaitCondition) -> Option<()> {
        let node = WaitNode::new(*id, condition);
        self.q.lock().push_back(node);
        Some(())
    }

    fn signal(&self) {
        for node in self.q.lock().drain(..) {
            if node.cond.is_given() && tls::task_data().wake(&node.id).is_none() {
                eprintln!("could not wake up task with id {}", node.id);
            }
        }
    }
}

impl Default for GenericWaitQueue {
    fn default() -> Self {
        Self {
            q: Mutex::new(VecDeque::new()),
        }
    }
}
