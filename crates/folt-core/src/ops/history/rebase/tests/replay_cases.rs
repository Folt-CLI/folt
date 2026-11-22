use super::*;

fn insert(
    facts: &mut RebaseFacts,
    template: &Commit,
    content: &[u8],
    parents: Vec<ObjectId>,
) -> ObjectId {
    let mut commit = template.clone();
    commit.parents = parents;
    commit.content = hash_blob(content);
    let id = hash_object(ObjectKind::Commit, &commit.encode_body().unwrap());
    facts.contents.insert(commit.content, content.to_vec());
    facts.commits.insert(id, commit);
    id
}

#[test]
fn clean_prefix_is_retained_when_the_next_replay_stops() {
    let home = TempHome::new().unwrap();
    let mut lock = home.home.try_lock().unwrap();
    let (_, _, _, copy, plan) = fixture(&home, &mut lock);
    let template = plan.facts.commits[&copy.commit_id.unwrap()].clone();
    let mut facts = plan.facts;
    facts.commits.clear();
    facts.contents.clear();
    let root = insert(&mut facts, &template, b"a\nb\nc\n", vec![]);
    let onto = insert(&mut facts, &template, b"A\nb\nc\n", vec![root]);
    let first = insert(&mut facts, &template, b"a\nb\nC\n", vec![root]);
    let second = insert(&mut facts, &template, b"x\nb\nC\n", vec![first]);
    let third = insert(&mut facts, &template, b"x\nB\nC\n", vec![second]);
    facts.range = [first, second, third].into_iter().collect();
    let ordered = replay::ordered(&facts).unwrap();
    assert_eq!(ordered, vec![first, second, third]);
    let result = replay::replay(&facts, onto, b"A\nb\nc\n".to_vec(), &ordered).unwrap();
    assert_eq!(result.replayed.len(), 1);
    let (tip, replayed, content) = &result.replayed[0];
    assert_eq!(result.tip, *tip);
    assert_eq!(replayed.parents, vec![onto]);
    assert_eq!(content, b"A\nb\nC\n");
    let mut expected = template.clone();
    expected.parents = vec![onto];
    expected.content = hash_blob(content);
    assert_eq!(*replayed, expected);
    let stop = result.stop.unwrap();
    assert_eq!(stop.current, second);
    assert_eq!(stop.remaining, vec![third]);
    assert_eq!(
        stop.marker_lines,
        vec![
            b"<<<<<<<\n".to_vec(),
            b"=======\n".to_vec(),
            b">>>>>>>\n".to_vec()
        ]
    );
    let resolved = replay::resolved(&facts.commits[&second], *tip, b"x\nb\nC\n".to_vec()).unwrap();
    let finished = replay::replay(&facts, resolved.0, resolved.2, &stop.remaining).unwrap();
    assert!(finished.stop.is_none());
    assert_eq!(finished.replayed[0].1.parents, vec![resolved.0]);
    assert_eq!(finished.content, b"x\nB\nC\n");
}
