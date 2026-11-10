//! A fixed-capacity circular buffer, used for the two ordered zones whose
//! cards come and go at both ends: the deck (drawn from the top, pitched cards
//! put on the bottom) and the stack (layers added and resolved at the top).
//!
//! `head` is the slot of the front element (always `< N`) and `len` the number
//! of elements, so an empty buffer and a full one are told apart without
//! giving up a slot. The capacity `N` can be any size from 1 to 128 — it need
//! not be a power of two, so a buffer can be sized exactly to what it must
//! hold (the deck is 40). Wrapping past the end of the backing array is a
//! compare-and-subtract rather than a modulo.

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RingBuffer<T: Copy, const N: usize> {
    items: [T; N],
    head: u8,
    len: u8,
}

// A general-purpose container: not every accessor has an engine caller yet.
#[allow(dead_code)]
impl<T: Copy, const N: usize> RingBuffer<T, N> {
    const CHECK_CAPACITY: () = assert!(N >= 1 && N <= 128, "RingBuffer capacity must be in 1..=128");

    /// An empty buffer. `fill` is only the value unused slots hold; it is
    /// never read back.
    pub const fn new(fill: T) -> Self {
        let () = Self::CHECK_CAPACITY;
        RingBuffer { items: [fill; N], head: 0, len: 0 }
    }

    /// The backing-array slot of the `i`th element counting from the front.
    /// `i` must be `< N` (callers only pass `i <= len`); since `head < N` too,
    /// one conditional subtraction wraps it.
    #[inline]
    fn slot(&self, i: usize) -> usize {
        let s = self.head as usize + i;
        if s >= N { s - N } else { s }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// The `i`th element counting from the front, if there is one.
    #[inline]
    pub fn get(&self, i: usize) -> Option<T> {
        (i < self.len()).then(|| self.items[self.slot(i)])
    }

    #[inline]
    pub fn front(&self) -> Option<T> {
        self.get(0)
    }

    #[inline]
    pub fn back(&self) -> Option<T> {
        self.len().checked_sub(1).and_then(|i| self.get(i))
    }

    /// Mutable access to the `i`th element counting from the front.
    #[inline]
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i < self.len() {
            let slot = self.slot(i);
            Some(&mut self.items[slot])
        } else {
            None
        }
    }

    /// Add `item` after the back. Returns `Err(item)` when the buffer is full.
    #[inline]
    pub fn try_push_back(&mut self, item: T) -> Result<(), T> {
        if self.len() == N {
            return Err(item);
        }
        let slot = self.slot(self.len());
        self.items[slot] = item;
        self.len += 1;
        Ok(())
    }

    /// Add `item` before the front. Returns `Err(item)` when the buffer is full.
    #[inline]
    pub fn try_push_front(&mut self, item: T) -> Result<(), T> {
        if self.len() == N {
            return Err(item);
        }
        self.head = if self.head == 0 { (N - 1) as u8 } else { self.head - 1 };
        self.len += 1;
        self.items[self.head as usize] = item;
        Ok(())
    }

    /// `try_push_back`, panicking when full.
    #[inline]
    pub fn push_back(&mut self, item: T) {
        if self.try_push_back(item).is_err() {
            panic!("RingBuffer of capacity {} is full", N);
        }
    }

    /// `try_push_front`, panicking when full.
    #[inline]
    pub fn push_front(&mut self, item: T) {
        if self.try_push_front(item).is_err() {
            panic!("RingBuffer of capacity {} is full", N);
        }
    }

    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        let item = self.items[self.head as usize];
        self.head = self.slot(1) as u8;
        self.len -= 1;
        Some(item)
    }

    #[inline]
    pub fn pop_back(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.len -= 1;
        Some(self.items[self.slot(self.len())])
    }

    /// Remove and return the `i`th element counting from the front, closing
    /// the gap by shifting the elements behind it forward. `None` when `i` is
    /// past the back.
    pub fn remove(&mut self, i: usize) -> Option<T> {
        let item = self.get(i)?;
        for j in i..self.len() - 1 {
            let next = self.items[self.slot(j + 1)];
            let slot = self.slot(j);
            self.items[slot] = next;
        }
        self.len -= 1;
        Some(item)
    }

    /// Remove and return the `i`th element counting from the front, filling
    /// its place with the back element: constant time, but the order is not
    /// kept. Use it only where the order is about to be thrown away anyway
    /// (a search followed by a shuffle). `None` when `i` is past the back.
    pub fn swap_remove(&mut self, i: usize) -> Option<T> {
        let item = self.get(i)?;
        let back = self.pop_back().expect("non-empty: it holds element i");
        if i < self.len() {
            let slot = self.slot(i);
            self.items[slot] = back;
        }
        Some(item)
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Every element, front to back.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = T> + ExactSizeIterator + '_ {
        (0..self.len()).map(move |i| self.items[self.slot(i)])
    }

    /// Rotate the elements to the start of the backing array and return them
    /// as one slice, front first, so they can be rearranged in place (e.g.
    /// shuffled).
    pub fn make_contiguous(&mut self) -> &mut [T] {
        let len = self.len();
        self.items.rotate_left(self.head as usize);
        self.head = 0;
        &mut self.items[..len]
    }
}

impl<T: Copy + std::fmt::Debug, const N: usize> std::fmt::Debug for RingBuffer<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents<const N: usize>(b: &RingBuffer<u8, N>) -> Vec<u8> {
        b.iter().collect()
    }

    #[test]
    fn push_and_pop_at_both_ends() {
        let mut b: RingBuffer<u8, 8> = RingBuffer::new(0);
        assert!(b.is_empty());
        b.push_back(1);
        b.push_back(2);
        b.push_front(0);
        assert_eq!(contents(&b), vec![0, 1, 2]);
        assert_eq!(b.front(), Some(0));
        assert_eq!(b.back(), Some(2));
        assert_eq!(b.pop_back(), Some(2));
        assert_eq!(b.pop_front(), Some(0));
        assert_eq!(b.pop_front(), Some(1));
        assert_eq!(b.pop_front(), None);
        assert_eq!(b.pop_back(), None);
    }

    #[test]
    fn holds_its_full_capacity_across_the_wrap() {
        let mut b: RingBuffer<u8, 4> = RingBuffer::new(0);
        // Walk the head well past several wraps of the u8 counters.
        for round in 0..200u8 {
            for i in 0..4 {
                b.push_back(round.wrapping_add(i));
            }
            assert_eq!(b.len(), 4);
            assert!(b.try_push_back(9).is_err());
            assert!(b.try_push_front(9).is_err());
            for i in 0..4 {
                assert_eq!(b.pop_front(), Some(round.wrapping_add(i)));
            }
            assert!(b.is_empty());
        }
    }

    #[test]
    fn non_power_of_two_capacity_wraps() {
        let mut b: RingBuffer<u8, 5> = RingBuffer::new(0);
        for round in 0..50u8 {
            for i in 0..5 {
                if i % 2 == 0 { b.push_back(i) } else { b.push_front(i) }
            }
            assert_eq!(contents(&b), vec![3, 1, 0, 2, 4]);
            assert!(b.try_push_back(9).is_err());
            assert!(b.try_push_front(9).is_err());
            assert_eq!(b.remove(round as usize % 5).is_some(), true);
            assert_eq!(b.len(), 4);
            while b.pop_back().is_some() {}
            assert!(b.is_empty());
            // Shift the head so the next round starts at a different slot.
            b.push_back(0);
            b.pop_front();
        }
    }

    #[test]
    fn make_contiguous_from_a_wrapped_odd_buffer() {
        let mut b: RingBuffer<u8, 6> = RingBuffer::new(0);
        for _ in 0..4 {
            b.push_back(0);
            b.pop_front();
        }
        for i in 1..=6 {
            b.push_back(i);
        }
        assert_eq!(b.make_contiguous(), &[1, 2, 3, 4, 5, 6]);
        b.pop_front();
        b.push_back(7);
        assert_eq!(contents(&b), vec![2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn remove_closes_the_gap() {
        let mut b: RingBuffer<u8, 8> = RingBuffer::new(0);
        // Start near the end of the backing array so the contents wrap.
        for _ in 0..6 {
            b.push_back(0);
            b.pop_front();
        }
        for i in 1..=5 {
            b.push_back(i);
        }
        assert_eq!(b.remove(1), Some(2));
        assert_eq!(contents(&b), vec![1, 3, 4, 5]);
        assert_eq!(b.remove(3), Some(5));
        assert_eq!(b.remove(0), Some(1));
        assert_eq!(contents(&b), vec![3, 4]);
        assert_eq!(b.remove(2), None);
    }

    #[test]
    fn swap_remove_fills_the_gap_with_the_back_element() {
        let mut b: RingBuffer<u8, 8> = RingBuffer::new(0);
        // Start the head near the end of the array so the elements wrap.
        for _ in 0..6 {
            b.push_back(0);
            b.pop_front();
        }
        for i in 1..=5 {
            b.push_back(i);
        }
        assert_eq!(b.swap_remove(1), Some(2));
        assert_eq!(contents(&b), vec![1, 5, 3, 4]);
        // The back element itself: nothing to move.
        assert_eq!(b.swap_remove(3), Some(4));
        assert_eq!(contents(&b), vec![1, 5, 3]);
        assert_eq!(b.swap_remove(0), Some(1));
        assert_eq!(contents(&b), vec![3, 5]);
        assert_eq!(b.swap_remove(2), None);
        assert_eq!(b.swap_remove(1), Some(5));
        assert_eq!(b.swap_remove(0), Some(3));
        assert!(b.is_empty());
        assert_eq!(b.swap_remove(0), None);
    }

    #[test]
    fn make_contiguous_keeps_the_order() {
        let mut b: RingBuffer<u8, 8> = RingBuffer::new(0);
        for _ in 0..5 {
            b.push_back(0);
            b.pop_front();
        }
        for i in 1..=6 {
            b.push_back(i);
        }
        assert_eq!(b.make_contiguous(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(contents(&b), vec![1, 2, 3, 4, 5, 6]);
        b.push_front(0);
        assert_eq!(contents(&b), vec![0, 1, 2, 3, 4, 5, 6]);
    }
}
