# flatvec

An indirection-collapsing container that generalizes from [nested](https://crates.io/crates/nested).

This crate provides a container `FlatVec` and two traits, `IntoFlat` and `FromFlat`. A `FlatVec` is parameterized on one type, and `IntoFlat` and `FromFlat` are both parameterized on two types. None of these type parameters need to be the same.

This permits collapsing indirections while also permitting minimal/zero-copy usage, as demonstrated in `examples/domain_name.rs`.

This interface also permits some interesting other applications, as in `examples/gzip.rs`.
