use alpathfinder::g::{GDoor, GMap};

#[test]
fn test_door_deserialization() {
    let json = r#"{
        "name": "test_map",
        "spawns": [[0.0, 0.0]],
        "doors": [
            [100.0, 200.0, 40.0, 50.0, "winterland", 0, 1],
            [300.0, 400.0, 30.0, 30.0, "bank", 2, 3, "key", "bank_key"],
            [500.0, 600.0, 20.0, 20.0, "cave", 1, 0, "closed", "extra_arg"],
            [],
            [10.0, 20.0],
            null
        ]
    }"#;

    let map: GMap = serde_json::from_str(json).expect("Should deserialize map with doors");
    assert_eq!(map.doors.len(), 3, "Should have parsed 3 valid doors, skipping invalid/empty ones");

    // Door 1: Standard 7 elements
    let d1 = &map.doors[0];
    assert_eq!(d1.x, 100.0);
    assert_eq!(d1.y, 200.0);
    assert_eq!(d1.width, 40.0);
    assert_eq!(d1.height, 50.0);
    assert_eq!(d1.map_to, "winterland");
    assert_eq!(d1.spawn_to, 0);
    assert_eq!(d1.spawn_from, 1);
    assert!(!d1.requires_key);
    assert_eq!(d1.key, None);

    // Door 2: Key door
    let d2 = &map.doors[1];
    assert_eq!(d2.x, 300.0);
    assert_eq!(d2.y, 400.0);
    assert_eq!(d2.map_to, "bank");
    assert_eq!(d2.spawn_to, 2);
    assert_eq!(d2.spawn_from, 3);
    assert!(d2.requires_key);
    assert_eq!(d2.key.as_deref(), Some("bank_key"));

    // Door 3: Non-key flag and extra args
    let d3 = &map.doors[2];
    assert_eq!(d3.x, 500.0);
    assert_eq!(d3.y, 600.0);
    assert_eq!(d3.map_to, "cave");
    assert_eq!(d3.spawn_to, 1);
    assert_eq!(d3.spawn_from, 0);
    assert!(!d3.requires_key);
    assert_eq!(d3.key, None);
}

#[test]
fn test_single_door_deserialize() {
    let json = r#"[10.0, 20.0, 30.0, 40.0, "tunnel", 5, 6]"#;
    let door: GDoor = serde_json::from_str(json).expect("Direct GDoor deserialize");
    assert_eq!(door.map_to, "tunnel");
    assert_eq!(door.spawn_to, 5);
    assert_eq!(door.spawn_from, 6);

    let invalid_json = r#"[10.0, 20.0]"#;
    let err = serde_json::from_str::<GDoor>(invalid_json);
    assert!(err.is_err());
}
