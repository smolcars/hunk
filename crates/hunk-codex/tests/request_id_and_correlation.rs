use hunk_codex::protocol::RequestId;
use hunk_codex::rpc::PendingRequestMap;
use hunk_codex::rpc::RequestIdGenerator;

#[test]
fn request_ids_are_monotonic() {
    let generator = RequestIdGenerator::new(41);
    let first = generator.next_request_id();
    let second = generator.next_request_id();
    assert_eq!(first, RequestId::Integer(41));
    assert_eq!(second, RequestId::Integer(42));
}

#[test]
fn pending_request_map_round_trip_for_integer_id() {
    let map = PendingRequestMap::default();
    let id = RequestId::Integer(7);

    assert!(map.is_empty());
    assert!(map.insert(&id, "value").is_none());
    assert_eq!(map.len(), 1);

    let removed = map.remove(&id);
    assert_eq!(removed, Some("value"));
    assert!(map.is_empty());
}

#[test]
fn pending_request_map_round_trip_for_string_id() {
    let map = PendingRequestMap::default();
    let id = RequestId::String("abc".to_string());

    assert!(map.insert(&id, "value").is_none());
    assert_eq!(map.len(), 1);

    let removed = map.remove(&id);
    assert_eq!(removed, Some("value"));
    assert!(map.is_empty());
}
