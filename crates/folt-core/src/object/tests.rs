#[test]
fn prefix_reader_never_requests_the_uncompressed_tail() {
    struct PrefixOnly {
        bytes: std::io::Cursor<Vec<u8>>,
    }
    impl std::io::Read for PrefixOnly {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            // Even asking for EOF after the prefix is forbidden here.
            assert!((self.bytes.position() as usize) < self.bytes.get_ref().len());
            std::io::Read::read(&mut self.bytes, output)
        }
    }
    let mut stored = b"\0blob 999999999\0".to_vec();
    stored.extend_from_slice(b"prefix");
    assert_eq!(
        super::read_prefix(
            PrefixOnly {
                bytes: std::io::Cursor::new(stored)
            },
            super::ObjectKind::Blob,
            6
        )
        .unwrap(),
        b"prefix"
    );
    let header_only = PrefixOnly {
        bytes: std::io::Cursor::new(b"\0blob 999999999\0".to_vec()),
    };
    assert!(
        super::read_prefix(header_only, super::ObjectKind::Blob, 0)
            .unwrap()
            .is_empty()
    );
}

use super::*;

fn commit(body: Option<&str>) -> Commit {
    Commit {
        content: hash_blob(b"content"),
        parents: vec![hash_blob(b"parent")],
        author: Author::new("Alice", Some("a@example.test")).unwrap(),
        time: CommitTime::from_unix_seconds(0).unwrap(),
        message: "subject".into(),
        body: body.map(str::to_owned),
        group: Some("group-1".into()),
    }
}

#[test]
fn framed_objects_round_trip() {
    for kind in [ObjectKind::Blob, ObjectKind::Commit, ObjectKind::Tag] {
        for body in [b"".as_slice(), b"hello\0world\n".as_slice()] {
            assert_eq!(decode(&encode(kind, body), kind, 100).unwrap(), body);
        }
    }
}

#[test]
fn wrong_kind_is_refused() {
    assert!(matches!(
        decode(&encode(ObjectKind::Blob, b"x"), ObjectKind::Tag, 10),
        Err(ObjectError::InvalidFrame)
    ));
}

#[test]
fn wrong_length_is_refused() {
    for framed in [b"blob 2\0x".as_slice(), b"blob 0\0x", b"blob 01\0x"] {
        assert!(matches!(
            decode(framed, ObjectKind::Blob, 10),
            Err(ObjectError::InvalidFrame)
        ));
    }
}

#[test]
fn decode_honors_limit() {
    assert!(matches!(
        decode(&encode(ObjectKind::Blob, b"ab"), ObjectKind::Blob, 1),
        Err(ObjectError::LimitExceeded)
    ));
}

#[test]
fn author_refuses_delimiters_and_newlines() {
    for name in ["a<b", "a>b", "a\nb"] {
        assert!(Author::new(name, None).is_err());
    }
    for email in ["a<b@example.test", "a>b@example.test", "a\nb@example.test"] {
        assert!(Author::new("Alice", Some(email)).is_err());
    }
}

#[test]
fn anonymous_has_no_email() {
    let author = Author::anonymous();
    assert_eq!(author.username(), "anonymous");
    assert_eq!(author.email(), None);
    assert_eq!(author.to_string(), "anonymous");
}

#[test]
fn commit_body_round_trip() {
    let original = commit(Some("First paragraph.\n\nSecond paragraph.\n"));
    assert_eq!(
        Commit::parse_body(&original.encode_body().unwrap()).unwrap(),
        original
    );
}

#[test]
fn empty_commit_body_is_none() {
    let original = commit(Some(""));
    assert_eq!(
        Commit::parse_body(&original.encode_body().unwrap()).unwrap(),
        commit(None)
    );
}

#[test]
fn tag_body_round_trip() {
    let tag = Tag {
        target: hash_blob(b"commit"),
        name: "v1".into(),
        tagger: Author::anonymous(),
        time: CommitTime::from_unix_seconds(0).unwrap(),
        message: "release".into(),
    };
    assert_eq!(Tag::parse_body(&tag.encode_body().unwrap()).unwrap(), tag);
}

#[test]
fn utc_time_text_is_canonical() {
    for (seconds, text) in [
        (0, "1970-01-01T00:00:00Z"),
        (-1, "1969-12-31T23:59:59Z"),
        (951782400, "2000-02-29T00:00:00Z"),
    ] {
        let time = CommitTime::from_unix_seconds(seconds).unwrap();
        assert_eq!(time.to_string(), text);
        assert_eq!(text.parse::<CommitTime>().unwrap(), time);
    }
}
