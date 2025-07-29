use core::{
    array,
    cell::UnsafeCell,
    fmt::Debug,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
    usize,
};
use linked_list_allocator::Heap;
use spin::Mutex;

use crate::{kernel::threading, serial_println};
// TODO use my thread safe mutex, however this currently does not work due to gkl policy
// TODO the current implementation has a bug presumably in read() causing (index related?), potentially recursive panics -> deadlocks, double faults, ...
// This NEEDS to be fixed before using it again. Should also rework tests, which do not currently catch this bug

pub struct ChunkedArrayQueue<const N: usize, T>
where
    T: Copy,
{
    /// this queue assumes n producers and 1 consumer
    /// currently T must be Copy to allow safely copying it into the buffer from &[T]. Might get changed later
    head: AtomicUsize, // consumer
    tail: AtomicUsize,            // producers
    reservated_tail: AtomicUsize, // producers
    lock: Mutex<()>,
    buffer: UnsafeCell<[MaybeUninit<T>; N]>,
}

impl<const N: usize, T: Copy> ChunkedArrayQueue<N, T> {
    pub fn new() -> Self {
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            reservated_tail: AtomicUsize::new(0),
            lock: Mutex::new(()),
            buffer: UnsafeCell::new(array::from_fn(|_| MaybeUninit::uninit())),
        }
    }

    pub fn push(&self, chunk: &[T]) {
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

    fn push_locked(&self, chunk: &[T]) {
        let lock = self.lock.lock();
        for chunk in chunk.chunks(N) {
            loop {
                if self._try_push_internal(chunk).is_ok() {
                    break;
                }
            }
        }
    }

    pub fn read(&self, buf: &mut [T]) -> Result<usize, QueueErr> {
        let current_tail = self.tail.load(Ordering::Acquire);
        let current_head = self.head.load(Ordering::Acquire);

        if current_head == current_tail {
            return Ok(0);
        }

        let tail_idx = Self::to_idx(current_tail);
        let head_idx = Self::to_idx(current_head);

        let n_read = (current_tail - current_head).min(buf.len());
        assert!(n_read <= N);

        let next_head_idx = Self::to_idx(current_head + n_read);

        let buffer = unsafe { self.get_buf() };

        // SAFETY: buffer can only be mutated thorugh push and if push is sound, all copied data must be initialized
        // n_read is <= min (buf.len(), buffer.len())
        if tail_idx <= head_idx || next_head_idx <= head_idx {
            // TODO check correctness
            // need to wrap around
            let first_n = N - head_idx;

            unsafe {
                ptr::copy_nonoverlapping(
                    buffer[head_idx..].as_ptr() as *const T,
                    buf[..first_n].as_mut_ptr(),
                    first_n,
                );
            }
            unsafe {
                ptr::copy_nonoverlapping(
                    buffer[..next_head_idx].as_ptr() as *const T,
                    buf[first_n..n_read].as_mut_ptr(),
                    n_read - first_n,
                );
            }
        } else {
            assert!(n_read <= tail_idx - head_idx);
            unsafe {
                ptr::copy_nonoverlapping(
                    buffer[head_idx..tail_idx].as_ptr() as *const T, // this panics sometimes??
                    buf[..n_read].as_mut_ptr(),
                    n_read,
                );
            }
        }
        if self.head.swap(current_head + n_read, Ordering::Release) != current_head {
            panic!("mpsc ChunkedArrayQueue was used with multiple consumers");
        }
        Ok(n_read)
    }

    pub fn try_push(&self, chunk: &[T]) -> Result<(), QueueErr> {
        if self.lock.is_locked() {
            return Err(QueueErr::IsLocked);
        }
        self._try_push_internal(chunk)
    }

    fn _try_push_internal(&self, chunk: &[T]) -> Result<(), QueueErr> {
        /// tries to push data and update tail. !This method (and thus all methods depending on it) may block / yield if another queue has reserved a slot beforehand and we must wait for it to update tail!
        assert!(self.tail.load(Ordering::Acquire) <= usize::MAX - chunk.len());
        if chunk.is_empty() {
            return Ok(());
        }
        if chunk.len() > N {
            return Err(QueueErr::SliceToLarge);
        }

        let old_tail = self
            .reservated_tail
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |mut tail| {
                tail += chunk.len();
                if (tail - self.head.load(Ordering::Relaxed)) > N {
                    // Relaxed should be fine here, as it does not matter if head is increased in the meantime
                    // open queue space is insufficient
                    return None;
                }
                Some(tail)
            })
            .map_err(|_| QueueErr::IsFull)?;

        // space is reserved and we do have enough space
        // can now copy data
        let start = Self::to_idx(old_tail);
        let end = Self::to_idx(old_tail + chunk.len());

        #[allow(unused_mut)]
        let mut buffer = unsafe { self.get_mut_buf() };

        // SAFETY: we copy <= chunk.len() Ts into buffer with chunk.len() <= buffer.len()
        if end <= start {
            let first_part = N - start;
            // end part
            unsafe {
                ptr::copy_nonoverlapping(
                    chunk[..first_part].as_ptr(),
                    buffer[start..N].as_mut_ptr() as *mut T,
                    first_part,
                );
            }
            unsafe {
                ptr::copy_nonoverlapping(
                    chunk[first_part..].as_ptr(),
                    buffer[..end].as_mut_ptr() as *mut T,
                    chunk.len() - first_part,
                );
            }
        } else {
            // no wraparound
            unsafe {
                ptr::copy_nonoverlapping(
                    chunk.as_ptr(),
                    buffer[start..end].as_mut_ptr() as *mut T,
                    chunk.len(),
                );
            }
        }

        // data is now copied and we must wait to update tail to reserved_tail + chunk.len()
        // another thread may be copying data in earlier slots and will update tail when it is done
        // this is necessary to ensure correctness of read()
        while self
            .tail
            .compare_exchange(
                old_tail,
                old_tail + chunk.len(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            threading::yield_now();
        }

        Ok(())
    }

    fn to_idx(val: usize) -> usize {
        val % N
    }

    unsafe fn get_buf(&self) -> &[MaybeUninit<T>] {
        unsafe { &*self.buffer.get() }
    }

    unsafe fn get_mut_buf(&self) -> &mut [MaybeUninit<T>] {
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

    pub fn len(&self) -> usize {
        N
    }
}

unsafe impl<const N: usize, T: Copy> Sync for ChunkedArrayQueue<N, T> {}
unsafe impl<const N: usize, T: Copy> Send for ChunkedArrayQueue<N, T> {}

impl<const N: usize, T: Copy + Debug> Debug for ChunkedArrayQueue<N, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ChunkedArrayQueue {{lock:{:#?}\tbuffer: {:#?}\tcap: {}}}",
            self.lock, self.buffer, N
        )
    }
}

impl<const N: usize, T: Copy> Default for ChunkedArrayQueue<N, T> {
    fn default() -> Self {
        Self::new()
    }
}

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
        let queue: ChunkedArrayQueue<10, u8> = ChunkedArrayQueue::new();

        assert!(queue.try_push(&[0; 11]).is_err());
        assert!(queue.try_push(&[42, 42]).is_ok());
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
        let queue: Arc<ChunkedArrayQueue<10, u8>> = Arc::new(ChunkedArrayQueue::new());

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
        assert!(handle2.unwrap().wait().is_ok()); // this deadlocks in alloc.lock() sometimes if it fails due to allocating a String in Result<Err(String)>????? WHY?? also other asserts affected???
        assert!(handle3.unwrap().wait().is_ok());
    }
}
