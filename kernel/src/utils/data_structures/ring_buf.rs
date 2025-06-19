use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
    usize,
};

use linked_list_allocator::Heap;

use crate::{kernel::threading, locks::thread_safe::Mutex};

pub struct ChunkedArrayQueue<const N: usize> {
    /// this queue assumes n producers and 1 consumer
    head: AtomicUsize, // consumer
    tail: AtomicUsize, // producer
    lock: Mutex<()>,
    buffer: UnsafeCell<[u8; N]>,
}

impl<const N: usize> ChunkedArrayQueue<N> {
    pub fn new() -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            lock: Mutex::new(()),
            buffer: UnsafeCell::new([0; N]),
        }
    }

    pub fn push(&self, chunk: &[u8]) {
        while self.lock.is_locked() {
            threading::yield_now();
        }
        if chunk.len() >= N {
            self.push_locked(chunk);
        }
        loop {
            if self.try_push(chunk).is_ok() {
                break;
            }
            threading::yield_now();
        }
    }

    fn head_idx(&self) -> usize {
        self.head.load(Ordering::Acquire) % N
    }

    fn tail_idx(&self) -> usize {
        self.tail.load(Ordering::Acquire) % N
    }

    fn push_locked(&self, chunk: &[u8]) {
        let lock = self.lock.lock();
        for chunk in chunk.chunks(N) {
            loop {
                if self.try_push(chunk).is_ok() {
                    continue;
                }
                threading::yield_now();
            }
        }
    }

    pub fn peek_chunk(&self) -> &[u8] {
        /// reads from head..tail
        let current_tail = self.tail.load(Ordering::Acquire);
        let current_head = self.head.load(Ordering::Acquire);

        if current_head == current_tail {
            return &[];
        }

        let tail_idx = Self::to_idx(current_tail);
        let head_idx = Self::to_idx(current_head);

        let mut buffer = unsafe { &*self.buffer.get() };
        if tail_idx <= head_idx {
            &buffer[head_idx..]
        } else {
            &buffer[head_idx..tail_idx]
        }
    }

    pub fn commit_chunk(&self, size: usize) -> Result<(), QueueErr> {
        /// commits a chunk (likely from peek_chunk) as having been used
        /// here we assume a single consumer
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if size >= N || head + size > tail {
            return Err(QueueErr::InvalidCommit);
        }
        self.head.fetch_add(size, Ordering::Release);
        Ok(())
    }

    pub fn read_into(&self, buffer: &mut [u8]) {
        todo!()
    }

    pub fn try_push(&self, chunk: &[u8]) -> Result<(), QueueErr> {
        if chunk.is_empty() {
            return Ok(());
        }
        if chunk.len() >= N {
            return Err(QueueErr::SliceToLarge);
        }
        let old_tail = self
            .tail
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |mut tail| {
                tail += chunk.len();
                if self.lock.is_locked() || tail - self.head.load(Ordering::Acquire) >= N {
                    // could probably use Relaxed here, as it does not matter if head is increased in the meantime
                    return None;
                }
                Some(tail)
            })
            .map_err(|_| QueueErr::IsFull)?;

        let start = Self::to_idx(old_tail);
        let end = Self::to_idx(start + chunk.len());

        let mut buffer = unsafe { &mut *self.buffer.get() };

        if end <= start {
            let first_part = N - start;
            // end part
            buffer[start..N].copy_from_slice(&chunk[..first_part]);
            buffer[..end].copy_from_slice(&chunk[first_part..]);
        } else {
            // no wraparound
            buffer[start..end].copy_from_slice(chunk);
        }
        Ok(())
    }

    fn to_idx(val: usize) -> usize {
        val % N
    }

    pub fn clear(&self) {
        todo!()
    }

    pub fn full(&self) -> bool {
        todo!()
    }

    pub fn empty(&self) -> bool {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueErr {
    IsFull,
    SliceToLarge,
    IsLocked,
    InvalidCommit,
}
