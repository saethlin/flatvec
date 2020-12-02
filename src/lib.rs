//! An indirection-collapsing container that generalizes [nested](https://crates.io/crates/nested).
//!
//! A `FlatVec` can be used like a `Vec<String>` or `Vec<Vec<u8>>`, but with a maximum of 2 heap
//! allocations instead of n + 1. With the `smallvec` feature enabled, these allocations are
//! delayed until the main storage capacity exceeds 16 and the length exceeds 2.
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
use core::convert::TryInto;
use core::fmt;
use core::iter;
use core::marker::PhantomData;
use core::str;
#[cfg(feature = "smallvec")]
use smallvec::SmallVec;

#[cfg(not(feature = "smallvec"))]
/// An indirection-collapsing container with minimal allocation
#[derive(Clone)]
pub struct FlatVec<T, IndexTy> {
    data: Box<[u8]>,
    data_len: usize,
    ends: Vec<IndexTy>,
    marker: PhantomData<T>,
}

#[cfg(feature = "smallvec")]
#[derive(Clone)]
pub struct FlatVec<T, IndexTy> {
    data: SmallVec<[u8; 16]>,
    ends: SmallVec<[IndexTy; 2]>,
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
            data: Default::default(),
            data_len: 0,
            ends: Default::default(),
            marker: PhantomData::default(),
        }
    }
}

impl<'a, T: 'a, IndexTy> FlatVec<T, IndexTy>
where
    IndexTy: TryInto<usize> + Copy + core::ops::Sub,
    usize: TryInto<IndexTy>,
    <IndexTy as TryInto<usize>>::Error: fmt::Debug,
    <usize as TryInto<IndexTy>>::Error: fmt::Debug,
{
    /// Create a new `FlatVec`
    #[inline]
    pub fn new() -> Self {
        Self {
            data: Default::default(),
            data_len: 0,
            ends: Default::default(),
            marker: PhantomData::default(),
        }
    }

    /// Returns the number of `T` in a `FlatVec<T>`
    #[inline]
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// Returns the number heap bytes used to store the elements of a `FlatVec`, but not the bytes
    /// used to store the indices
    #[inline]
    pub fn data_len(&self) -> usize {
        self.data_len
    }

    #[inline]
    pub fn data_capacity(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the len is 0
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ends.len() == 0
    }

    /// Removes the `index`th element of a `FlatVec`.
    /// This function is `O(self.len() + self.data_len())`
    #[inline]
    pub fn remove(&mut self, index: usize) {
        let end = self.ends[index].try_into().unwrap();
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
            let change: usize = (*end).try_into().unwrap() - removed_len;
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
        iter::once(0usize)
            .chain(self.ends.iter().copied().map(|v| v.try_into().unwrap()))
            .zip(self.ends.iter().copied().map(|v| v.try_into().unwrap()))
            .map(move |(start, end)| Dest::from_flat(&self.data[start..end]))
    }
}

/// A wrapper over the innards of a `FlatVec` which exposes mutating operations which cannot
/// corrupt other elements during a push
#[cfg(not(feature = "smallvec"))]
pub struct Storage<'a> {
    data: &'a mut Box<[u8]>,
    data_len: &'a mut usize,
}

#[cfg(feature = "smallvec")]
pub struct Storage<'a>(&'a mut SmallVec<[u8; 16]>);

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
    /// In general, due to missed optimizations, this is slower than calling `allocate` with the
    /// exact size required.
    #[inline]
    pub fn extend<Iter, T>(&mut self, iter: Iter)
    where
        Iter: IntoIterator<Item = T>,
        T: Borrow<u8>,
    {
        let mut iter = iter.into_iter().map(|b| *b.borrow());

        for (out, val) in self.data.iter_mut().skip(*self.data_len).zip(&mut iter) {
            *out = val;
            *self.data_len += 1;
        }

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
            let empty = std::mem::swap(self.data, &mut data.into_boxed_slice());
            std::mem::forget(empty);
        }
    }

    #[inline]
    pub fn extend_exact<OuterIter, InnerIter, T>(&mut self, iter: OuterIter)
    where
        OuterIter: IntoIterator<Item = T, IntoIter = InnerIter>,
        InnerIter: Iterator<Item = T> + ExactSizeIterator,
        T: Borrow<u8>,
    {
        let iter = iter.into_iter();
        let chunk = self.allocate(iter.len());
        let mut iter = iter.into_iter();
        for (out, val) in chunk.iter_mut().zip(&mut iter) {
            *out = *val.borrow();
        }
        assert!(iter.next().is_none());
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
        store.extend(self.into_iter());
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
