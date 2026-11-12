//! Placement facts and targets for decision tests.

use crate::id::FolioId;
use crate::object::hash_blob;
use crate::placement::{Observation, PlacementBase, PlacementKind, Signature};
use crate::placement_rules::{MoveTarget, PlacementFacts};
use crate::ref_name::RefName;
use crate::registry::{self, PlacementPath};

pub(crate) fn facts() -> PlacementFacts {
    let hash = hash_blob(b"base");
    let signature = Signature {
        size: 4,
        mtime: None,
        ctime: None,
        inode: None,
    };
    PlacementFacts {
        placement: registry::Placement {
            path: PlacementPath::resolve(std::env::temp_dir().join("folt-unit-facts")).unwrap(),
            id: FolioId::from_bytes([1; 16]),
            kind: PlacementKind::Copy,
            head: "topic".into(),
            commit_id: Some(hash),
            base: Some(PlacementBase {
                hash,
                signature: signature.clone(),
            }),
            staged: None,
        },
        observation: Observation::File { signature },
        placement_hash: Some(hash),
        tip: Some(hash),
    }
}

pub(crate) fn target() -> MoveTarget {
    MoveTarget {
        branch: RefName::parse("topic").unwrap(),
        commit: hash_blob(b"commit"),
        content: hash_blob(b"incoming"),
    }
}

#[cfg(test)]
pub(crate) use relocation::RelocateFixture;

#[cfg(test)]
mod relocation {
    use super::*;
    use crate::home::HomeLock;
    use crate::placement::{self, PlacementOps};
    use crate::placement_rules::{RelocatePlan, gather_relocate_target, judge_relocate};
    use crate::registry::Registry;
    use crate::test_support::TempHome;

    pub(crate) struct RelocateFixture {
        pub(crate) root: std::path::PathBuf,
        pub(crate) home: crate::home::Home,
        pub(crate) row: registry::Placement,
        _directory: crate::test_support::TempDir,
    }

    impl RelocateFixture {
        pub(crate) fn new(directory: bool) -> (Self, HomeLock, Registry) {
            let TempHome {
                root,
                home,
                _directory,
            } = TempHome::new().unwrap();
            let lock = home.try_lock().unwrap();
            let mut registry = Registry::open_or_create(&home, &lock).unwrap();
            let mut row = facts().placement;
            row.path = registry::PlacementPath::resolve(root.join("source")).unwrap();
            let fs = crate::placement_fs::PlacementFs::new(&home);
            if directory {
                std::fs::create_dir(row.path.to_path_buf()).unwrap();
            }
            registry
                .write(&lock, |tx| {
                    tx.add_folio(&registry::NewFolio {
                        id: row.id,
                        format: registry::FolioFormat {
                            layout: crate::layout::FOLIO_LAYOUT_VERSION,
                            algorithm: crate::config::formats::OBJECT_ALGORITHM.into(),
                        },
                    })?;
                    for index in 0..if directory { 3 } else { 1 } {
                        let mut child = row.clone();
                        if directory {
                            child.path = registry::PlacementPath::resolve(
                                row.path.to_path_buf().join(format!("file-{index}")),
                            )?;
                        }
                        std::fs::write(child.path.to_path_buf(), b"edited content").unwrap();
                        child.staged = Some(placement::StagedBlob {
                            hash: hash_blob(b"stage"),
                            size: 5,
                        });
                        tx.insert_placement(&child)?;
                        if !directory {
                            row = child;
                        }
                    }
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                fs.read(row.id, &row.to_placement().1).unwrap() == Observation::Directory,
                directory
            );
            (
                Self {
                    root,
                    home,
                    _directory,
                    row,
                },
                lock,
                registry,
            )
        }

        pub(crate) fn facts(&self) -> PlacementFacts {
            let fs = crate::placement_fs::PlacementFs::new(&self.home);
            let observation = fs.read(self.row.id, &self.row.to_placement().1).unwrap();
            let placement_hash = matches!(observation, Observation::File { .. })
                .then(|| fs.hash_placement(&self.row.to_placement().1).unwrap());
            PlacementFacts {
                placement: self.row.clone(),
                observation,
                placement_hash,
                tip: None,
            }
        }

        pub(crate) fn plan(&self, registry: &Registry) -> RelocatePlan {
            let fs = crate::placement_fs::PlacementFs::new(&self.home);
            let to =
                registry::PlacementPath::resolve_directory(self.root.join("destination")).unwrap();
            let target = gather_relocate_target(&self.row, &to, &fs, registry).unwrap();
            judge_relocate(&self.facts(), &target).unwrap()
        }
    }
}
