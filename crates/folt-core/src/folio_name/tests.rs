use super::*;

#[test]
fn stem_slug_boundaries() {
    for (stem, expected) in [
        ("A-Z_a.b09", "A-Z_a.b09"),
        ("Hello  world!", "Hello-world"),
        ("a日本語 /\t🙂b", "a-b"),
        ("a - b", "a---b"),
        ("..--.bashrc.-", "bashrc"),
        ("__", "__"),
        ("..._---", "_"),
        ("日本語A日本語", "A"),
        (&"a".repeat(63), &"a".repeat(63)),
        (&"g".repeat(64), &"g".repeat(64)),
        (&"g".repeat(100), &"g".repeat(64)),
        (&format!("{}.-.z", "g".repeat(61)), &"g".repeat(61)),
        (&format!("{}日本語z", "g".repeat(63)), &"g".repeat(63)),
        (&format!("..{}..", "g".repeat(64)), &"g".repeat(64)),
    ] {
        let name = FolioName::from_stem(stem).unwrap();
        assert_eq!(name.as_str(), expected, "{stem:?}");
        assert_eq!(FolioName::parse(expected), Ok(name));
    }
}

#[test]
fn unusable_stems_require_an_explicit_name() {
    for (stem, expected) in [
        ("", StemError::Empty),
        ("日本語", StemError::Empty),
        ("🙂é", StemError::Empty),
        (" .- /\t", StemError::Empty),
        (&"a".repeat(64), StemError::HexDigest),
        (&"ABCDEF0123456789".repeat(4), StemError::HexDigest),
        (&"a".repeat(65), StemError::HexDigest),
        (&format!("{}g", "a".repeat(64)), StemError::HexDigest),
    ] {
        assert_eq!(FolioName::from_stem(stem), Err(expected), "{stem:?}");
        assert!(expected.to_string().contains("--name"));
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_stem_requires_an_explicit_name() {
    use std::os::unix::ffi::OsStrExt;

    let stem = OsStr::from_bytes(b"valid-prefix\xff");
    assert_eq!(FolioName::from_stem(stem), Err(StemError::NonUtf8));
}

#[cfg(windows)]
#[test]
fn non_utf8_stem_requires_an_explicit_name() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let stem = OsString::from_wide(&[0x61, 0xd800]);
    assert_eq!(FolioName::from_stem(stem), Err(StemError::NonUtf8));
}

#[test]
fn suffix_boundaries() {
    for (base, n, expected) in [
        ("Note", 0, "Note_0".to_owned()),
        ("Note", 1, "Note_1".to_owned()),
        ("_", u32::MAX, "__4294967295".to_owned()),
        (&"g".repeat(62), 1, format!("{}_1", "g".repeat(62))),
        (&"g".repeat(64), 1, format!("{}_1", "g".repeat(62))),
        (&"g".repeat(64), 10, format!("{}_10", "g".repeat(61))),
        (
            &"g".repeat(64),
            u32::MAX,
            format!("{}_4294967295", "g".repeat(53)),
        ),
        (
            &format!("{}.-z", "g".repeat(61)),
            1,
            format!("{}_1", "g".repeat(61)),
        ),
        (
            &format!("{}-.z", "g".repeat(60)),
            1,
            format!("{}_1", "g".repeat(60)),
        ),
        (&"a".repeat(63), 1, format!("{}_1", "a".repeat(62))),
    ] {
        let base = FolioName::parse(base).unwrap();
        let name = base.with_suffix(n);
        assert_eq!(name.as_str(), expected, "{base:?}, {n}");
        assert_eq!(FolioName::parse(&expected), Ok(name));
    }
}

#[test]
fn grammar_boundaries() {
    for value in [
        "a",
        "Logo",
        "logo",
        "a.b_c-d",
        "_x",
        "x_",
        "a..b",
        "1234",
        "deadbeef",
        "con",
        "NUL.txt",
        "550e8400-e29b-41d4-a716-446655440000",
        &"g".repeat(64),
        &"a".repeat(63),
    ] {
        let name = FolioName::parse(value).unwrap();
        assert_eq!(name.as_str(), value);
        assert_eq!(name.as_ref(), value);
        assert_eq!(name.to_string(), value);
    }
    for value in [
        "",
        ".a",
        "a.",
        "-a",
        "a-",
        ".",
        "-",
        "a@b",
        "a#b",
        "a:b",
        "a/b",
        "a b",
        "a\tb",
        "a\nb",
        "a\rb",
        "é",
        &"a".repeat(64),
        &"A".repeat(64),
        &"g".repeat(65),
        &"a".repeat(65),
    ] {
        assert!(FolioName::parse(value).is_err(), "{value:?}");
    }
    // 65 hex digits fail the length rule, rather than the digest rule.
    assert_eq!(
        FolioName::parse(&"a".repeat(65)),
        Err(FolioNameError::Length)
    );
    assert_ne!(
        FolioName::parse("Logo").unwrap(),
        FolioName::parse("logo").unwrap()
    );
}
