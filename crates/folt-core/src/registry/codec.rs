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

pub(super) struct EncodedConflict {
    pub operation: Option<&'static str>,
    pub other: Option<Vec<u8>>,
    pub markers: Option<Vec<u8>>,
    pub pre_merge: Option<Vec<u8>>,
}

pub(super) fn encode_conflict(
    state: Option<&crate::placement::ConflictState>,
) -> Result<EncodedConflict, RegistryError> {
    let Some(state) = state else {
        return Ok(EncodedConflict {
            operation: None,
            other: None,
            markers: None,
            pre_merge: None,
        });
    };
    let mut markers = Vec::new();
    for line in &state.marker_lines {
        let length = u64::try_from(line.len()).map_err(|_| RegistryError::InvalidConflictState)?;
        markers.extend_from_slice(&length.to_be_bytes());
        markers.extend_from_slice(line);
    }
    Ok(EncodedConflict {
        operation: Some(match state.operation {
            crate::placement::ConflictOperation::Merge => "merge",
        }),
        other: Some(state.other.as_bytes().to_vec()),
        markers: Some(markers),
        pre_merge: Some(state.pre_merge.as_bytes().to_vec()),
    })
}

pub(super) fn decode_conflict(
    operation: Option<String>,
    other: Option<Vec<u8>>,
    markers: Option<Vec<u8>>,
    pre_merge: Option<Vec<u8>>,
) -> Result<Option<crate::placement::ConflictState>, RegistryError> {
    use crate::placement::{ConflictOperation, ConflictState};
    let invalid = || RegistryError::InvalidConflictState;
    let (operation, other, markers, pre_merge) = match (operation, other, markers, pre_merge) {
        (None, None, None, None) => return Ok(None),
        (Some(operation), Some(other), Some(markers), Some(pre_merge)) => {
            (operation, other, markers, pre_merge)
        }
        _ => return Err(invalid()),
    };
    let operation = match operation.as_str() {
        "merge" => ConflictOperation::Merge,
        _ => return Err(invalid()),
    };
    let mut remaining = markers.as_slice();
    let mut marker_lines = Vec::new();
    while !remaining.is_empty() {
        let (length, rest) = remaining.split_at_checked(8).ok_or_else(invalid)?;
        let length = u64::from_be_bytes(length.try_into().map_err(|_| invalid())?);
        let length = usize::try_from(length).map_err(|_| invalid())?;
        let (line, rest) = rest.split_at_checked(length).ok_or_else(invalid)?;
        marker_lines.push(line.to_vec());
        remaining = rest;
    }
    Ok(Some(ConflictState {
        operation,
        other: ObjectId::from_bytes(other.try_into().map_err(|_| invalid())?),
        marker_lines,
        pre_merge: ObjectId::from_bytes(pre_merge.try_into().map_err(|_| invalid())?),
    }))
}
