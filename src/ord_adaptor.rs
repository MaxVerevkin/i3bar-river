//! Newtypes to use floating numbers in sorting algs

use std::cmp::Ordering;

/// Defaults to `Ordering::Less` if the wrapped value can't be compared
#[derive(PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DefaultLess<T: PartialOrd>(pub T);

// Not ideal, but `Ord` requires `Eq`, so...
impl<T: PartialOrd> Eq for DefaultLess<T> {}

#[allow(clippy::derive_ord_xor_partial_ord)]
impl<T: PartialOrd> Ord for DefaultLess<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Less)
    }
}
