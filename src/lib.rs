//! An indirection-collapsing container that generalizes [nested](https://crates.io/crates/nested).
//!
//! A `FlatVec` can be used like a `Vec<String>` or `Vec<Vec<u8>>`, but with a maximum of 2 heap
//! allocations instead of n + 1.
//!
//! Insertion into and retrieval from a `FlatVec` is mediated by two traits, `IntoFlat` and
//! `FromFlat`, which are both parameterized on two types. The simplest way to use this crate is to
//! `impl IntoFlat<T> for T` and `impl FromFlat<'_, T> for T` for some type `T` in your crate.
//!
//! But in general, the interface is `impl IntoFlat<Flattened> for Source` and `impl FromFlat<'a,
//! Dest> for Flattened`. An owned or borrowed type `Source` is effectively serialized into a
//! `FlatVec`, then an instance of a type that defines its ability to be constructed from a
//! `FlatVec<Flattened>` can be produced, possibly borrowing the data in the `FlatVec` to return a
//! specialized type that does not copy the data stored in the `FlatVec` to present a view of it.
//!
//! This interface is extremely powerful and essentially amounts to in-memory serialization and
//! conversion all in one. For example, a user can construct a `FlatVec` that compreses all of its
//! elements with gzip. I'm not saying that's a good idea. But you can.

#![forbid(unsafe_code)]

use core::borrow::Borrow;
use core::convert::{TryFrom, TryInto};
use core::fmt;
use core::iter;
use core::marker::PhantomData;
use core::str;

/// An indirection-collapsing container with minimal allocation
#[derive(Clone)]
pub struct FlatVec<T, IndexTy> {
    data: Box<[u8]>,
    data_len: usize,
    ends: Vec<IndexTy>,
    marker: PhantomData<T>,
}

impl<T, IndexTy> fmt::Debug for FlatVec<T, IndexTy>
where
    IndexTy: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("FlatVec")
            .field("data", &&self.data[..self.data_len])
            .field("ends", &self.ends)
            .finish()
    }
}

impl<T> Default for FlatVec<T, usize> {
    #[inline]
    fn default() -> Self {
        Self {
            data: Box::default(),
            data_len: 0,
            ends: Vec::default(),
            marker: PhantomData::default(),
        }
    }
}

impl<'a, T: 'a, IndexTy> FlatVec<T, IndexTy>
where
    IndexTy: TryFrom<usize> + Copy + core::ops::Sub,
    usize: TryFrom<IndexTy>,
    <IndexTy as TryFrom<usize>>::Error: fmt::Debug,
    <usize as TryFrom<IndexTy>>::Error: fmt::Debug,
{
    /// Create a new `FlatVec`
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Box::default(),
            data_len: 0,
            ends: Vec::default(),
            marker: PhantomData::default(),
        }
    }

    /// Returns the number of `T` in a `FlatVec<T>`
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// Returns the number heap bytes used to store the elements of a `FlatVec`, but not the bytes
    /// used to store the indices
    #[inline]
    #[must_use]
    pub fn data_len(&self) -> usize {
        self.data_len
    }

    #[inline]
    #[must_use]
    pub fn data_capacity(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the len is 0
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ends.len() == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        self.data_len = 0;
        self.ends.clear();
    }

    /// Removes the `index`th element of a `FlatVec`.
    /// This function is `O(self.len() + self.data_len())`
    #[inline]
    pub fn remove(&mut self, index: usize) {
        let end: usize = self.ends[index].try_into().unwrap();
        let start = if index == 0 {
            0
        } else {
            self.ends[index - 1].try_into().unwrap()
        };
        self.data.copy_within(end.., start);
        self.ends.remove(index);
        let removed_len = end - start;
        self.data_len -= removed_len;
        self.ends.iter_mut().skip(index).for_each(|end| {
            let change = usize::try_from(*end).unwrap() - removed_len;
            *end = change.try_into().unwrap();
        });
    }

    /// Appends an element to the back of the collection
    #[inline]
    pub fn push<Source>(&mut self, input: Source)
    where
        Source: IntoFlat<T>,
    {
        input.into_flat(Storage {
            data: &mut self.data,
            data_len: &mut self.data_len,
        });
        self.ends.push(self.data_len.try_into().unwrap());
    }

    /// Construct a `Dest` from the `index`th element's stored representation
    #[inline]
    #[must_use]
    pub fn get<Dest: 'a>(&'a self, index: usize) -> Option<Dest>
    where
        Dest: FromFlat<'a, T>,
    {
        if index >= self.ends.len() {
            None
        } else {
            let end = self.ends[index].try_into().unwrap();
            let start = if index == 0 {
                0
            } else {
                self.ends[index - 1].try_into().unwrap()
            };
            Some(Dest::from_flat(&self.data[start..end]))
        }
    }

    /// Returns an iterator that constructs a `Dest` from each element's stored representation
    #[inline]
    pub fn iter<Dest: 'a>(&'a self) -> impl Iterator<Item = Dest> + 'a
    where
        Dest: FromFlat<'a, T>,
    {
        iter::once(0)
            .chain(self.ends.iter().copied().map(|v| v.try_into().unwrap()))
            .zip(self.ends.iter().copied().map(|v| v.try_into().unwrap()))
            .map(move |(start, end)| Dest::from_flat(&self.data[start..end]))
    }
}

/// A wrapper over the innards of a `FlatVec` which exposes mutating operations which cannot
/// corrupt other elements during a push
pub struct Storage<'a> {
    data: &'a mut Box<[u8]>,
    data_len: &'a mut usize,
}

impl Storage<'_> {
    #[inline(never)]
    fn allocate_slow_path(&mut self, requested: usize) {
        let mut data = std::mem::take(self.data).into_vec();
        data.resize(std::cmp::max(requested + data.len(), 2 * data.len()), 0);
        *self.data = data.into_boxed_slice();
    }

    #[inline]
    pub fn allocate(&mut self, requested: usize) -> &mut [u8] {
        self.reserve(requested);
        let old_len = *self.data_len;
        *self.data_len += requested;
        &mut self.data[old_len..old_len + requested]
    }

    /// Reserves capacity for at least `len` additional bytes
    #[inline]
    pub fn reserve(&mut self, requested: usize) {
        if self.data.len() < *self.data_len + requested {
            self.allocate_slow_path(requested);
        }
    }

    /// Inserts the bytes described by `iter`
    /// In general, due to missed optimizations, this is ~2x slower than calling `allocate` when
    /// the exact size of the inserted object is known.
    #[inline]
    pub fn extend<Iter, T>(&mut self, iter: Iter)
    where
        Iter: IntoIterator<Item = T>,
        T: Borrow<u8>,
    {
        let iter = iter.into_iter();

        // When the iterator provides a precise size hint, use the allocate interface.
        // This path is ~5x faster than the one below
        let hint = iter.size_hint();
        if Some(hint.0) == hint.1 {
            let data = self.allocate(hint.0);
            for (src, dst) in iter.into_iter().zip(data.iter_mut()) {
                *dst = *src.borrow();
            }
            return;
        }

        let mut iter = iter.map(|b| *b.borrow());

        // Insert as many elements as possible into already-allocated space
        for (out, val) in self.data.iter_mut().skip(*self.data_len).zip(&mut iter) {
            *out = val;
            *self.data_len += 1;
        }

        // If there are elements remaining in the iterator, allocate space for them
        if let Some(val) = iter.next() {
            let mut data = std::mem::take(self.data).into_vec();
            data.push(val);
            *self.data_len += 1;

            for val in iter {
                data.push(val);
                *self.data_len += 1;
            }

            if data.capacity() > data.len() {
                data.resize(data.capacity(), 0);
            }
            let mut data = data.into_boxed_slice();
            std::mem::swap(self.data, &mut data);
            std::mem::forget(data);
        }
    }
}

/// Implement `IntoFlat<Flattened> for Source` to insert a `Source` into a `FlatVec<Flattened>`
pub trait IntoFlat<Flattened> {
    fn into_flat(self, storage: Storage);
}

/// Implement `FromFlat<'a, Flattened> for Dest` to get a `Dest` from a `FlatVec<Flattened>`
pub trait FromFlat<'a, Flattened> {
    fn from_flat(data: &'a [u8]) -> Self;
}

impl IntoFlat<String> for String {
    #[inline]
    fn into_flat(self, mut store: Storage) {
        store.extend(self.bytes());
    }
}

impl FromFlat<'_, String> for String {
    #[inline]
    fn from_flat(data: &[u8]) -> Self {
        String::from_utf8(data.to_vec()).unwrap()
    }
}

impl IntoFlat<String> for &str {
    #[inline]
    fn into_flat(self, mut store: Storage) {
        store.extend(self.bytes());
    }
}

impl<'a> FromFlat<'a, String> for &'a str {
    #[inline]
    fn from_flat(data: &'a [u8]) -> &'a str {
        str::from_utf8(&data).unwrap()
    }
}

impl<Iter, T> IntoFlat<Vec<u8>> for Iter
where
    Iter: IntoIterator<Item = T>,
    T: Borrow<u8>,
{
    #[inline]
    fn into_flat(self, mut store: Storage) {
        store.extend(self);
    }
}

impl FromFlat<'_, Vec<u8>> for Vec<u8> {
    #[inline]
    fn from_flat(data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }
}

impl<'a> FromFlat<'a, Vec<u8>> for &'a [u8] {
    #[inline]
    fn from_flat(data: &'a [u8]) -> &'a [u8] {
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_get() {
        let mut names = FlatVec::default();
        names.push("Cerryl");
        names.push("Jeslek".to_string());
        assert_eq!(names.get(0), Some("Cerryl"));
        assert_eq!(names.get(1), Some("Jeslek"));
        assert_eq!(names.get::<String>(2), None);
    }

    #[test]
    fn iter() {
        let mut names = FlatVec::default();
        names.push("Cerryl".to_string());
        names.push("Jeslek".to_string());
        let as_vec = names.iter::<String>().collect::<Vec<_>>();
        assert_eq!(as_vec, vec!["Cerryl".to_string(), "Jeslek".to_string()]);
    }
}
