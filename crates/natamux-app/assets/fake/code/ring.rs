//! A fixed size ring of order ids.

use std::fmt;

/// How many orders the ring holds.
pub const SIZE: usize = 8;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Ring<T> {
    items: Vec<Option<T>>,
    head: usize,
    len: usize,
}

impl<T: Clone> Ring<T> {
    pub fn new() -> Self {
        Ring { items: vec![None; SIZE], head: 0, len: 0 }
    }

    /// False when the ring is full.
    pub fn push(&mut self, item: T) -> bool {
        if self.len == SIZE {
            return false;
        }
        self.items[(self.head + self.len) % SIZE] = Some(item);
        self.len += 1;
        true
    }

    pub fn pop(&mut self) -> Option<T> {
        let item = self.items.get_mut(self.head)?.take()?;
        self.head = (self.head + 1) % SIZE;
        self.len -= 1;
        Some(item)
    }
}

impl<T> fmt::Display for Ring<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ring of {}/{SIZE}", self.len)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_the_order() {
        let mut ring = super::Ring::new();
        assert!(ring.push('a') && ring.push('b'));
        assert_eq!(ring.pop(), Some('a'));
    }
}
