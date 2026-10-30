//! Hidden tests: FIFO behavior and drop accounting. The agent never sees this file.

use std::cell::Cell;
use std::rc::Rc;

use ringbuf::RingBuffer;

#[test]
fn fifo_order_and_len() {
    let mut rb = RingBuffer::with_capacity(3);
    assert!(rb.is_empty());
    assert_eq!(rb.capacity(), 3);
    rb.push(1).expect("room");
    rb.push(2).expect("room");
    assert_eq!(rb.len(), 2);
    assert_eq!(rb.pop(), Some(1));
    assert_eq!(rb.pop(), Some(2));
    assert_eq!(rb.pop(), None);
    assert!(rb.is_empty());
}

#[test]
fn full_buffer_gives_the_value_back() {
    let mut rb = RingBuffer::with_capacity(2);
    rb.push("a".to_owned()).expect("room");
    rb.push("b".to_owned()).expect("room");
    assert_eq!(rb.push("c".to_owned()), Err("c".to_owned()));
    assert_eq!(rb.len(), 2);
}

#[test]
fn wraps_around_many_times() {
    let mut rb = RingBuffer::with_capacity(5);
    let mut next_in = 0u32;
    let mut next_out = 0u32;
    for round in 0..1000 {
        for _ in 0..(round % 5 + 1) {
            if rb.push(next_in).is_ok() {
                next_in += 1;
            }
        }
        for _ in 0..(round % 3 + 1) {
            if let Some(v) = rb.pop() {
                assert_eq!(v, next_out);
                next_out += 1;
            }
        }
        assert_eq!(rb.len(), usize::try_from(next_in - next_out).unwrap_or(0));
    }
}

#[test]
fn capacity_one_works() {
    let mut rb = RingBuffer::with_capacity(1);
    for i in 0..10 {
        rb.push(i).expect("room");
        assert_eq!(rb.push(99), Err(99));
        assert_eq!(rb.pop(), Some(i));
    }
}

#[test]
#[should_panic]
fn zero_capacity_panics() {
    let _ = RingBuffer::<u8>::with_capacity(0);
}

/// Counts drops through a shared cell.
struct Tracked(Rc<Cell<usize>>);

impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn drops_remaining_elements_exactly_once() {
    let drops = Rc::new(Cell::new(0));
    {
        let mut rb = RingBuffer::with_capacity(4);
        for _ in 0..4 {
            assert!(rb.push(Tracked(Rc::clone(&drops))).is_ok());
        }
        // Wrap around so live elements straddle the end of the storage.
        drop(rb.pop());
        drop(rb.pop());
        assert!(rb.push(Tracked(Rc::clone(&drops))).is_ok());
        assert_eq!(drops.get(), 2, "popped elements are dropped by the caller");
    }
    assert_eq!(drops.get(), 5, "three left in the buffer, each dropped once");
}

#[test]
fn rejected_push_does_not_drop_or_leak() {
    let drops = Rc::new(Cell::new(0));
    let mut rb = RingBuffer::with_capacity(1);
    assert!(rb.push(Tracked(Rc::clone(&drops))).is_ok());
    let back = rb.push(Tracked(Rc::clone(&drops)));
    assert_eq!(drops.get(), 0);
    drop(back);
    drop(rb);
    assert_eq!(drops.get(), 2);
}

#[test]
fn works_with_heap_values() {
    let mut rb = RingBuffer::with_capacity(3);
    for round in 0..100 {
        rb.push(vec![round; 16]).expect("room");
        if round % 2 == 1 {
            assert_eq!(rb.pop().map(|v| v.len()), Some(16));
            assert_eq!(rb.pop().map(|v| v.len()), Some(16));
        }
    }
}
