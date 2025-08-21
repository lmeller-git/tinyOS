use core::fmt::Debug;

use bitflags::bitflags;
use conquer_once::spin::OnceCell;
use crossbeam::queue::ArrayQueue;
use hashbrown::HashMap;

use crate::{
    arch::interrupt,
    kernel::threading::{
        self,
        task::TaskID,
        tls,
        wait::{
            condition::WaitCondition,
            queues::{KeyBoardQueue, TimeWaitQueue, WaitQueue},
        },
    },
    serial_println,
    sync::locks::{Mutex, RwLock},
};

pub mod condition;
pub mod queues;

pub static MESSAGE_QUEUE: OnceCell<ArrayQueue<WaitEvent<u64>>> = OnceCell::uninit();

pub fn init() {
    MESSAGE_QUEUE.init_once(|| ArrayQueue::new(20));
}

pub struct WaitObserver<'a> {
    queues: RwLock<HashMap<QueueType, &'a dyn WaitQueue>>,
}

impl<'a> WaitObserver<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_queue(
        &mut self,
        queue: &'a dyn WaitQueue,
        queue_type: QueueType,
    ) -> Option<&'a dyn WaitQueue> {
        self.queues
            .write()
            .try_insert(queue_type, queue)
            .ok()
            .map(|v| &**v)
    }

    pub fn remove_queue(&mut self, queue_type: &QueueType) {
        self.queues.write().remove_entry(queue_type);
    }

    pub fn process_signals(&self, msg: &ArrayQueue<WaitEvent<u64>>) {
        let map = self.queues.read();
        while let Some(s) = msg.pop() {
            map.get(&s.event_type).map(|&q| q.signal());
        }
    }

    pub fn enqueue(&self, id: &TaskID, queue_data: &[QueuTypeCondition]) {
        let map = self.queues.read();
        // TODO proper atomic multi enqueue
        for q in queue_data {
            if q.cond.is_given() {
                serial_println!("early return");
                return;
            }
            map.get(&q.q_type)
                .map(|queue| queue.enqueue(id, q.cond.clone()));
        }
        drop(map);
        loop {
            if interrupt::without_interrupts(|| {
                queue_data.iter().any(|c| c.cond.is_given())
                    || tls::task_data().try_block(id).is_none()
            }) {
                threading::yield_now();
            } else {
                return;
            }
        }
        serial_println!("early return 2");
    }
}

impl Default for WaitObserver<'_> {
    fn default() -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
        }
    }
}

unsafe impl Send for WaitObserver<'_> {}
unsafe impl Sync for WaitObserver<'_> {}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum QueueType {
    Timer,
    KeyBoard,
}

#[derive(Debug, PartialEq, Eq)]
pub struct QueuTypeCondition {
    pub q_type: QueueType,
    pub cond: WaitCondition,
}

impl QueuTypeCondition {
    pub fn new(q_type: QueueType) -> Self {
        Self {
            q_type,
            cond: WaitCondition::None,
        }
    }

    pub fn with_cond(q_type: QueueType, cond: WaitCondition) -> Self {
        Self { q_type, cond }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct WaitEvent<D: Copy + Debug + PartialEq + Eq> {
    pub event_type: QueueType,
    pub data: D,
}
