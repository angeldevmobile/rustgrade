//! Fixed-capacity FIFO queue for the audio callback.

use std::marker::PhantomData;

pub struct RingBuffer<T> {
    _todo: PhantomData<T>,
}

impl<T> RingBuffer<T> {
    /// A buffer holding up to `capacity` elements. Panics if `capacity` is 0.
    pub fn with_capacity(capacity: usize) -> Self {
        let _ = capacity;
        todo!()
    }

    /// Appends `value`, or gives it back if the buffer is full.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let _ = value;
        todo!()
    }

    /// Removes the oldest element.
    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    pub fn capacity(&self) -> usize {
        todo!()
    }
}
