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

#[cfg(feature = "smallvec")]
use smallvec::SmallVec;
use std::marker::PhantomData;

#[cfg(not(feature = "smallvec"))]
/// An indirection-collapsing container with minimal allocation
#[derive(Clone)]
pub struct FlatVec<T> {
    data: Vec<u8>,
    ends: Vec<usize>,
    marker: PhantomData<T>,
}

#[cfg(feature = "smallvec")]
#[derive(Clone)]
pub struct FlatVec<T> {
    data: SmallVec<[u8; 16]>,
    ends: SmallVec<[usize; 2]>,
    marker: PhantomData<T>,
}

impl<T> std::fmt::Debug for FlatVec<T> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("FlatVec")
            .field("data", &self.data)
            .field("ends", &self.ends)
            .finish()
    }
}

impl<T> Default for FlatVec<T> {
    #[inline]
    fn default() -> Self {
        Self {
            data: Default::default(),
            ends: Default::default(),
            marker: PhantomData::default(),
        }
    }
}

impl<'a, T: 'a> FlatVec<T> {
    /// Create a new `FlatVec`
    #[inline]
    pub fn new() -> Self {
        Self::default()
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
        let end = self.ends[index];
        let start = if index == 0 { 0 } else { self.ends[index - 1] };
        self.data.drain(start..end);
        self.ends.remove(index);
        let len = end - start;
        self.ends.iter_mut().skip(index).for_each(|end| *end -= len);
    }

    /// Appends an element to the back of the collection
    #[inline]
    pub fn push<Source>(&mut self, input: Source)
    where
        Source: IntoFlat<T>,
    {
        input.into_flat(Storage(&mut self.data));
        self.ends.push(self.data.len());
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
            let end = self.ends[index];
            let start = if index == 0 { 0 } else { self.ends[index - 1] };
            Some(Dest::from_flat(&self.data[start..end]))
        }
    }

    /// Returns an iterator that constructs a `Dest` from each element's stored representation
    #[inline]
    pub fn iter<Dest: 'a>(&'a self) -> impl Iterator<Item = Dest> + 'a
    where
        Dest: FromFlat<'a, T>,
    {
        std::iter::once(&0usize)
            .chain(self.ends.iter())
            .zip(self.ends.iter())
            .map(move |(&start, &end)| Dest::from_flat(&self.data[start..end]))
    }
}

/// A wrapper over the innards of a `FlatVec` which exposes mutating operations which cannot
/// corrupt other elements during a push
#[cfg(not(feature = "smallvec"))]
pub struct Storage<'a>(&'a mut Vec<u8>);

#[cfg(feature = "smallvec")]
pub struct Storage<'a>(&'a mut SmallVec<[u8; 16]>);

impl Storage<'_> {
    /// Reserves capacity for at least `len` additional bytes
    #[inline]
    pub fn reserve(&mut self, len: usize) {
        self.0.reserve(len);
    }

    /// Inserts the bytes described by `iter`
    #[inline]
    pub fn extend<Iter, T>(&mut self, iter: Iter)
    where
        Iter: IntoIterator<Item = T>,
        T: std::borrow::Borrow<u8>,
    {
        self.0.extend(iter.into_iter().map(|b| *b.borrow()));
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
        std::str::from_utf8(&data).unwrap()
    }
}

impl<Iter, T> IntoFlat<Vec<u8>> for Iter
where
    Iter: IntoIterator<Item = T>,
    T: std::borrow::Borrow<u8>,
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
        let mut names = FlatVec::new();
        names.push("Cerryl");
        names.push("Jeslek".to_string());
        assert_eq!(names.get(0), Some("Cerryl"));
        assert_eq!(names.get(1), Some("Jeslek"));
        assert_eq!(names.get::<String>(2), None);
    }

    #[test]
    fn iter() {
        let mut names = FlatVec::new();
        names.push("Cerryl".to_string());
        names.push("Jeslek".to_string());
        let as_vec = names.iter::<String>().collect::<Vec<_>>();
        assert_eq!(as_vec, vec!["Cerryl".to_string(), "Jeslek".to_string()]);
    }
}
