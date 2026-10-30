//! Reference solution: proves the hidden tests are passable. Never shown to agents.

use std::mem::MaybeUninit;

pub struct RingBuffer<T> {
    slots: Box<[MaybeUninit<T>]>,
    /// Index of the oldest element.
    head: usize,
    len: usize,
}

impl<T> RingBuffer<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be at least 1");
        Self {
            slots: (0..capacity).map(|_| MaybeUninit::uninit()).collect(),
            head: 0,
            len: 0,
        }
    }

    fn index(&self, offset: usize) -> usize {
        (self.head + offset) % self.slots.len()
    }

    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.len == self.slots.len() {
            return Err(value);
        }
        let i = self.index(self.len);
        self.slots[i].write(value);
        self.len += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let i = self.head;
        self.head = self.index(1);
        self.len -= 1;
        // SAFETY: slot `i` was the oldest of `len` live slots, so it was
        // initialized by `push`; moving `head` past it marks it dead, so it is
        // read exactly once.
        Some(unsafe { self.slots[i].assume_init_read() })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
}

impl<T> Drop for RingBuffer<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}
