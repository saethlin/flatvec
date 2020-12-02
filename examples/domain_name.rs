use flatvec::{FlatVec, FromFlat, IntoFlat, Storage};

fn main() {
    for _ in 0..10_000 {
        let mut names: FlatVec<_, u32> = FlatVec::new();
        for _ in 0..1_000 {
            names.push(DomainNameRef {
                ttl: 60,
                time_seen: 31415,
                name: &b"google.com"[..],
            });
        }
        let mut count = 0;
        for name in names.iter::<DomainNameRef>() {
            assert_eq!(
                name,
                DomainNameRef {
                    ttl: 60,
                    time_seen: 31415,
                    name: &b"google.com"[..],
                }
            );
            count += 1;
        }
        assert_eq!(count, 1_000);
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct DomainName {
    ttl: u32,
    time_seen: u32,
    name: Vec<u8>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct DomainNameRef<'a> {
    ttl: u32,
    time_seen: u32,
    name: &'a [u8],
}

impl FromFlat<'_, DomainName> for DomainName {
    fn from_flat(data: &[u8]) -> Self {
        assert!(data.len() >= 8);
        Self {
            time_seen: u32::from_ne_bytes([data[0], data[1], data[2], data[3]]),
            ttl: u32::from_ne_bytes([data[4], data[5], data[6], data[7]]),
            name: data[8..].to_vec(),
        }
    }
}

impl<'a> FromFlat<'a, DomainName> for DomainNameRef<'a> {
    fn from_flat(data: &'a [u8]) -> Self {
        assert!(data.len() >= 8);
        Self {
            time_seen: u32::from_ne_bytes([data[0], data[1], data[2], data[3]]),
            ttl: u32::from_ne_bytes([data[4], data[5], data[6], data[7]]),
            name: &data[8..],
        }
    }
}

impl IntoFlat<DomainName> for DomainNameRef<'_> {
    fn into_flat(self, mut store: Storage) {
        store.extend(
            self.time_seen
                .to_ne_bytes()
                .iter()
                .chain(self.ttl.to_ne_bytes().iter())
                .chain(self.name.iter()),
        );
        /*
        let data = store.allocate(4 + 4 + self.name.len());
        data[..4].copy_from_slice(&self.time_seen.to_ne_bytes());
        data[4..8].copy_from_slice(&self.ttl.to_ne_bytes());
        data[8..].copy_from_slice(self.name);
        */
    }
}
