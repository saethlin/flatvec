use flatvec::{FlatVec, FromFlat, IntoFlat, Storage};

fn main() {
    let mut names: FlatVec<_, u32, u8, 3> = FlatVec::new();
    // Insert an owned type, extract a borrowed type
    names.push(DomainName {
        ttl: 60,
        time_seen: 31415,
        name: b"google.com".to_vec(),
    });
    assert_eq!(
        names.get::<DomainNameRef>(0).unwrap(),
        DomainNameRef {
            ttl: 60,
            time_seen: 31415,
            name: &b"google.com"[..],
        }
    );

    names.clear();
    // Insert a borrowed type, extract an owned type
    // With the same FlatVec
    names.push(DomainNameRef {
        ttl: 60,
        time_seen: 31415,
        name: &b"google.com"[..],
    });
    assert_eq!(
        names.get::<DomainName>(0).unwrap(),
        DomainName {
            ttl: 60,
            time_seen: 31415,
            name: b"google.com".to_vec(),
        }
    );
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

impl FromFlat<'_, u8, DomainName> for DomainName {
    fn from_flat(data: &[u8]) -> Self {
        assert!(data.len() >= 8);
        Self {
            time_seen: u32::from_ne_bytes([data[0], data[1], data[2], data[3]]),
            ttl: u32::from_ne_bytes([data[4], data[5], data[6], data[7]]),
            name: data[8..].to_vec(),
        }
    }
}

impl<'a> FromFlat<'a, u8, DomainName> for DomainNameRef<'a> {
    fn from_flat(data: &'a [u8]) -> Self {
        assert!(data.len() >= 8);
        Self {
            time_seen: u32::from_ne_bytes([data[0], data[1], data[2], data[3]]),
            ttl: u32::from_ne_bytes([data[4], data[5], data[6], data[7]]),
            name: &data[8..],
        }
    }
}

impl IntoFlat<u8, DomainName> for DomainName {
    fn into_flat(self, mut store: Storage<u8>) {
        let data = store.allocate(4 + 4 + self.name.len());
        data[..4].copy_from_slice(&self.time_seen.to_ne_bytes());
        data[4..8].copy_from_slice(&self.ttl.to_ne_bytes());
        data[8..].copy_from_slice(&self.name);
    }
}

impl IntoFlat<u8, DomainName> for DomainNameRef<'_> {
    fn into_flat(self, mut store: Storage<u8>) {
        let data = store.allocate(4 + 4 + self.name.len());
        data[..4].copy_from_slice(&self.time_seen.to_ne_bytes());
        data[4..8].copy_from_slice(&self.ttl.to_ne_bytes());
        data[8..].copy_from_slice(self.name);
    }
}
