use std::io;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};

use super::{sys, IoUring};

pub struct CompletionQueue<'ring> {
    ring: NonNull<sys::io_uring>,
    _marker: PhantomData<&'ring mut IoUring>,
}

impl<'ring> CompletionQueue<'ring> {
    pub(crate) fn new(ring: &'ring IoUring) -> CompletionQueue<'ring> {
        CompletionQueue {
            ring: NonNull::from(&ring.ring),
            _marker: PhantomData,
        }
    }

    pub fn peek_for_cqe(&mut self) -> Option<CompletionQueueEvent<'_>> {
        unsafe {
            let mut cqe = MaybeUninit::uninit();
            let count = sys::io_uring_peek_batch_cqe(self.ring.as_ptr(), cqe.as_mut_ptr(), 1);
            if count > 0 {
                Some(CompletionQueueEvent::new(
                    self.ring,
                    &mut *cqe.assume_init(),
                ))
            } else {
                None
            }
        }
    }

    pub fn wait_for_cqe(&mut self) -> io::Result<CompletionQueueEvent<'_>> {
        self.wait_for_cqes(1)
    }

    pub fn wait_for_cqes(&mut self, count: usize) -> io::Result<CompletionQueueEvent<'_>> {
        unsafe {
            let mut cqe = MaybeUninit::uninit();

            let _: i32 = resultify!(sys::io_uring_wait_cqes(
                self.ring.as_ptr(),
                cqe.as_mut_ptr(),
                count as _,
                ptr::null(),
                ptr::null(),
            ))?;

            Ok(CompletionQueueEvent::new(
                self.ring,
                &mut *cqe.assume_init(),
            ))
        }
    }

    pub fn wait_for_cqes_multi(
        &mut self,
        count: usize,
    ) -> io::Result<Vec<CompletionQueueEvent<'_>>> {
        unsafe {
            let mut cqe = ptr::null_mut();

            let _: i32 = resultify!(sys::io_uring_wait_cqes(
                self.ring.as_ptr(),
                &mut cqe,
                count as _,
                ptr::null(),
                ptr::null(),
            ))?;

            let mut entries = Vec::with_capacity(count);

            for n in 0..count as isize {
                let cqe = CompletionQueueEvent::new(self.ring, &mut *cqe.offset(n));
                entries.push(cqe);
            }

            Ok(entries)
        }
    }
}

unsafe impl<'ring> Send for CompletionQueue<'ring> {}
unsafe impl<'ring> Sync for CompletionQueue<'ring> {}

pub struct CompletionQueueEvent<'a> {
    ring: NonNull<sys::io_uring>,
    cqe: &'a mut sys::io_uring_cqe,
}

impl<'a> CompletionQueueEvent<'a> {
    pub(crate) fn new(
        ring: NonNull<sys::io_uring>,
        cqe: &'a mut sys::io_uring_cqe,
    ) -> CompletionQueueEvent<'a> {
        CompletionQueueEvent { ring, cqe }
    }

    pub fn is_timeout(&self) -> bool {
        self.cqe.user_data == sys::LIBURING_UDATA_TIMEOUT
    }

    pub fn user_data(&self) -> u64 {
        self.cqe.user_data as u64
    }

    pub fn result(&self) -> io::Result<usize> {
        resultify!(self.cqe.res)
    }

    pub fn raw(&self) -> &sys::io_uring_cqe {
        self.cqe
    }

    pub fn raw_mut(&mut self) -> &mut sys::io_uring_cqe {
        self.cqe
    }
}

impl<'a> Drop for CompletionQueueEvent<'a> {
    fn drop(&mut self) {
        unsafe {
            sys::rust_io_uring_cq_advance(self.ring.as_ptr(), 1);
        }
    }
}

unsafe impl<'a> Send for CompletionQueueEvent<'a> {}
unsafe impl<'a> Sync for CompletionQueueEvent<'a> {}
