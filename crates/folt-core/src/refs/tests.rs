use super::*;

#[test]
fn lower_ref_error_converts_variant_for_variant() {
    let errors = [
        LowerRefError::InvalidName("bad name".into()),
        LowerRefError::InvalidRef(PathBuf::from("invalid")),
        LowerRefError::Conflict {
            existing: PathBuf::from("existing"),
        },
        LowerRefError::Io {
            path: PathBuf::from("io"),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        },
    ];
    for lower in errors {
        let text = lower.to_string();
        // Compare all fields before moving the lower error into its conversion.
        let expected = match &lower {
            LowerRefError::InvalidName(name) => (0, name.clone(), None),
            LowerRefError::InvalidRef(path) => (1, path.to_str().unwrap().into(), None),
            LowerRefError::Conflict { existing } => (2, existing.to_str().unwrap().into(), None),
            LowerRefError::Io { path, source } => (
                3,
                path.to_str().unwrap().into(),
                Some((source.kind(), source.to_string())),
            ),
        };
        let upper = RefError::from(lower);
        assert_eq!(upper.to_string(), text);
        match upper {
            RefError::InvalidName(name) => assert_eq!(expected, (0, name, None)),
            RefError::InvalidRef(path) => {
                assert_eq!(expected, (1, path.to_str().unwrap().into(), None))
            }
            RefError::Conflict { existing } => {
                assert_eq!(expected, (2, existing.to_str().unwrap().into(), None))
            }
            RefError::Io { path, source } => assert_eq!(
                expected,
                (
                    3,
                    path.to_str().unwrap().into(),
                    Some((source.kind(), source.to_string()))
                )
            ),
            error => panic!("unexpected converted variant: {error:?}"),
        }
    }
}

#[test]
fn ref_error_keeps_the_upper_variants() {
    let id = ObjectId::from_bytes([1; 32]);
    let stale = RefError::Stale { actual: Some(id) };
    assert!(matches!(stale, RefError::Stale { actual: Some(actual) } if actual == id));
    assert_eq!(
        stale.to_string(),
        format!("stale ref; actual value: {:?}", Some(id))
    );
    let missing = RefError::MissingTarget(id);
    assert!(matches!(missing, RefError::MissingTarget(actual) if actual == id));
    assert_eq!(missing.to_string(), format!("missing ref target: {id}"));
    let wrong = RefError::WrongKind {
        id,
        actual: ObjectKind::Blob,
    };
    assert!(matches!(
        wrong,
        RefError::WrongKind {
            actual: ObjectKind::Blob,
            ..
        }
    ));
    assert_eq!(wrong.to_string(), format!("invalid ref target {id}: Blob"));
}
