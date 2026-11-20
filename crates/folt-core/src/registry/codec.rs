use crate::fsx::Signature;
use crate::id::ObjectId;
use crate::placement::{PlacementBase, StagedBlob};
use crate::placement_path::RegistryError;

pub(super) struct EncodedBase {
    pub(super) hash: Option<Vec<u8>>,
    pub(super) size: Option<i64>,
    pub(super) mtime: Option<i64>,
    pub(super) ctime: Option<i64>,
    pub(super) inode: Option<i64>,
}

pub(super) fn encode_base(base: Option<&PlacementBase>) -> Result<EncodedBase, RegistryError> {
    let Some(base) = base else {
        return Ok(EncodedBase {
            hash: None,
            size: None,
            mtime: None,
            ctime: None,
            inode: None,
        });
    };
    let size = i64::try_from(base.signature.size)
        .map_err(|_| RegistryError::SizeOutOfRange(base.signature.size))?;
    let time = |value: Option<i128>| value.and_then(|n| i64::try_from(n).ok());
    Ok(EncodedBase {
        hash: Some(base.hash.as_bytes().to_vec()),
        size: Some(size),
        mtime: time(base.signature.mtime),
        ctime: time(base.signature.ctime),
        inode: base.signature.inode.map(|n| n as i64),
    })
}
pub(super) fn decode_base(
    hash: Option<Vec<u8>>,
    size: Option<i64>,
    mtime: Option<i64>,
    ctime: Option<i64>,
    inode: Option<i64>,
) -> Result<Option<PlacementBase>, RegistryError> {
    let Some(hash) = hash else {
        return Ok(None);
    };
    let hash: [u8; 32] = hash
        .try_into()
        .map_err(|_| RegistryError::InvalidBaseHash)?;
    let size = size.ok_or(RegistryError::InvalidBaseSignature)?;
    Ok(Some(PlacementBase {
        hash: ObjectId::from_bytes(hash),
        signature: Signature {
            size: u64::try_from(size).map_err(|_| RegistryError::InvalidBaseSignature)?,
            mtime: mtime.map(i128::from),
            ctime: ctime.map(i128::from),
            inode: inode.map(|n| n as u64),
        },
    }))
}
pub(super) fn encode_staged(
    staged: Option<StagedBlob>,
) -> Result<(Option<Vec<u8>>, Option<i64>), RegistryError> {
    match staged {
        None => Ok((None, None)),
        Some(staged) => Ok((
            Some(staged.hash.as_bytes().to_vec()),
            Some(
                i64::try_from(staged.size)
                    .map_err(|_| RegistryError::SizeOutOfRange(staged.size))?,
            ),
        )),
    }
}
pub(super) fn decode_staged(
    hash: Option<Vec<u8>>,
    size: Option<i64>,
) -> Result<Option<StagedBlob>, RegistryError> {
    match (hash, size) {
        (None, None) => Ok(None),
        (Some(hash), Some(size)) => Ok(Some(StagedBlob {
            hash: ObjectId::from_bytes(hash.try_into().map_err(|_| RegistryError::InvalidStaged)?),
            size: u64::try_from(size).map_err(|_| RegistryError::InvalidStaged)?,
        })),
        _ => Err(RegistryError::InvalidStaged),
    }
}

pub(super) fn read_id(bytes: Vec<u8>) -> Result<[u8; 16], RegistryError> {
    bytes.try_into().map_err(|_| RegistryError::InvalidId)
}
pub(super) fn prefix_successor(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut upper = prefix.to_vec();
    for index in (0..upper.len()).rev() {
        if upper[index] != u8::MAX {
            upper[index] += 1;
            upper.truncate(index + 1);
            return Some(upper);
        }
    }
    None
}
pub(super) fn prefix_boundary(prefix: &[u8]) -> Vec<u8> {
    let mut boundary = prefix.to_vec();
    #[cfg(unix)]
    boundary.push(b'/');
    #[cfg(windows)]
    boundary.extend_from_slice(&[b'\\', 0]);
    boundary
}
