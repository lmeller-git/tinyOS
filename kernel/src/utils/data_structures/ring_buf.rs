use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
    usize,
};

use linked_list_allocator::Heap;

use crate::{kernel::threading, serial_println};
//TODO use my thread safe mutex, however this currently does not work due to gkl policy
use spin::Mutex;

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
        if chunk.len() > N {
            self.push_locked(chunk);
            return;
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
                if self._try_push_internal(chunk).is_ok() {
                    break;
                }
            }
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize, QueueErr> {
        let current_tail = self.tail.load(Ordering::Acquire);
        let current_head = self.head.load(Ordering::Acquire);

        if current_head == current_tail {
            return Ok(0);
        }

        let tail_idx = Self::to_idx(current_tail);
        let head_idx = Self::to_idx(current_head);

        let n_read = (current_tail - current_head).min(buf.len());

        let next_head_idx = Self::to_idx(current_head + n_read);

        let buffer = unsafe { self.get_buf() };

        if tail_idx <= head_idx && next_head_idx <= tail_idx {
            // need to wrap around
            let first_n = N - head_idx;
            buf[..first_n].copy_from_slice(&buffer[head_idx..]);
            buf[first_n..n_read].copy_from_slice(&buffer[..next_head_idx]);
        } else {
            buf[..n_read].copy_from_slice(&buffer[head_idx..tail_idx])
        }
        if self.head.swap(current_head + n_read, Ordering::Release) != current_head {
            panic!("mpsc ChunkedArrayQueue was used with multiple consumers");
        }
        Ok(n_read)
    }

    pub fn peek_chunk(&self) -> &[u8] {
        /// reads from head..tail without wrapping
        let current_tail = self.tail.load(Ordering::Acquire);
        let current_head = self.head.load(Ordering::Acquire);

        if current_head == current_tail {
            return &[];
        }

        let tail_idx = Self::to_idx(current_tail);
        let head_idx = Self::to_idx(current_head);

        let buffer = unsafe { self.get_buf() };
        if tail_idx <= head_idx {
            &buffer[head_idx..]
        } else {
            &buffer[head_idx..tail_idx]
        }
    }

    pub fn use_chunk(&self, size: usize) -> Result<(), QueueErr> {
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

    pub fn try_push(&self, chunk: &[u8]) -> Result<(), QueueErr> {
        if self.lock.is_locked() {
            return Err(QueueErr::IsLocked);
        }
        self._try_push_internal(chunk)
    }

    pub fn _try_push_internal(&self, chunk: &[u8]) -> Result<(), QueueErr> {
        assert!(self.tail.load(Ordering::Acquire) <= usize::MAX - chunk.len());
        if chunk.is_empty() {
            return Ok(());
        }
        if chunk.len() > N {
            return Err(QueueErr::SliceToLarge);
        }
        let old_tail = self
            .tail
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |mut tail| {
                tail += chunk.len();
                if (tail - self.head.load(Ordering::Acquire)) > N {
                    // could probably use Relaxed here, as it does not matter if head is increased in the meantime
                    return None;
                }
                Some(tail)
            })
            .map_err(|_| QueueErr::IsFull)?;

        let start = Self::to_idx(old_tail);
        let end = Self::to_idx(start + chunk.len());

        let mut buffer = unsafe { self.get_mut_buf() };

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

    unsafe fn get_buf(&self) -> &[u8] {
        unsafe { &*self.buffer.get() }
    }

    unsafe fn get_mut_buf(&self) -> &mut [u8] {
        unsafe { &mut *self.buffer.get() }
    }

    pub fn clear(&self) {
        self.head
            .store(self.tail.load(Ordering::Acquire), Ordering::Release);
    }

    pub fn is_full(&self) -> bool {
        self.tail.load(Ordering::Acquire) - self.head.load(Ordering::Acquire) == N
    }

    pub fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Acquire) == self.head.load(Ordering::Acquire)
    }

    pub fn current_bytes(&self) -> usize {
        self.tail.load(Ordering::Acquire) - self.head.load(Ordering::Acquire)
    }
}

unsafe impl<const N: usize> Sync for ChunkedArrayQueue<N> {}
unsafe impl<const N: usize> Send for ChunkedArrayQueue<N> {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueErr {
    IsFull,
    SliceToLarge,
    IsLocked,
    InvalidCommit,
}

mod tests {
    use alloc::sync::Arc;
    use os_macros::kernel_test;

    use super::*;

    #[kernel_test]
    fn spsc() {
        let queue: ChunkedArrayQueue<10> = ChunkedArrayQueue::new();

        assert!(queue.try_push(&[0; 11]).is_err());
        assert!(queue.try_push(&[42, 42]).is_ok());
        assert_eq!(*queue.peek_chunk(), [42, 42]);
        assert!(queue.try_push(&[42; 9]).is_err());

        queue.push(&[42; 4]);
        let mut buffer = [0; 10];
        assert_eq!(queue.read(&mut buffer).unwrap(), 6);
        assert_eq!(buffer, [42, 42, 42, 42, 42, 42, 0, 0, 0, 0]);
        assert!(queue.is_empty());

        queue.push(&[10; 10]);
        assert!(queue.is_full());
        assert_eq!(queue.read(&mut buffer).unwrap(), 10);
        assert_eq!(buffer, [10; 10]);
        assert!(queue.is_empty());

        queue.push(&[1]);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[kernel_test]
    fn mpsc() {
        let queue: Arc<ChunkedArrayQueue<10>> = Arc::new(ChunkedArrayQueue::new());

        let handle1 = {
            let queue = queue.clone();
            threading::spawn(move || {
                for _ in 0..100 {
                    queue.push(&[1; 8]);
                }
                queue.push(&[1; 20]);
                queue.push(&[3]);
            })
        };
        let handle2 = {
            let queue = queue.clone();
            threading::spawn(move || {
                for _ in 0..100 {
                    queue.push(&[2; 8]);
                }
                queue.push(&[2; 20]);
                queue.push(&[4]);
            })
        };

        let handle3 = threading::spawn(move || {
            let mut nums = [0; 4];
            let mut buffer = [0; 20];
            loop {
                if let Ok(n) = queue.read(&mut buffer) {
                    for num in &buffer[..n] {
                        nums[(num - 1) as usize] += 1;
                    }
                }
                if nums[2] == 1 && nums[3] == 1 {
                    assert_eq!(nums[0], 820);
                    assert_eq!(nums[1], 820);
                    break;
                }
            }
        });

        assert!(handle1.unwrap().wait().is_ok());
        assert!(handle2.unwrap().wait().is_ok());
        assert!(handle3.unwrap().wait().is_ok());
    }
}
