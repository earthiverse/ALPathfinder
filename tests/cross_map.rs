use alpathfinder::g::{GData, GDoor, GGeometry, GMap};
use alpathfinder::{clear, get_path_nodes, prepare_map};
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_cross_map_navigation_and_early_exit() {
    let _lock = TEST_MUTEX.lock().unwrap();
    clear();

    let mut g = GData::default();

    // Map A
    let mut map_a = GMap::default();
    map_a.spawns = vec![[0.0, 0.0], [50.0, 0.0]].into_boxed_slice();
    map_a.doors = vec![GDoor {
        x: 50.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        map_to: "map_b".to_string(),
        spawn_to: 0,
        spawn_from: 1,
        requires_key: false,
        key: None,
    }];
    g.maps.insert("map_a".to_string(), map_a);
    let mut geom_a = GGeometry::default();
    geom_a.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("map_a".to_string(), geom_a);

    // Map B
    let mut map_b = GMap::default();
    map_b.spawns = vec![[0.0, 0.0], [50.0, 0.0]].into_boxed_slice();
    map_b.doors = vec![GDoor {
        x: 50.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        map_to: "map_c".to_string(),
        spawn_to: 0,
        spawn_from: 1,
        requires_key: false,
        key: None,
    }];
    g.maps.insert("map_b".to_string(), map_b);
    let mut geom_b = GGeometry::default();
    geom_b.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("map_b".to_string(), geom_b);

    // Map C (Destination)
    let mut map_c = GMap::default();
    map_c.spawns = vec![[0.0, 0.0], [50.0, 0.0]].into_boxed_slice();
    g.maps.insert("map_c".to_string(), map_c);
    let mut geom_c = GGeometry::default();
    geom_c.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("map_c".to_string(), geom_c);

    // Map D (Disconnected Island)
    let mut map_d = GMap::default();
    map_d.spawns = vec![[0.0, 0.0]].into_boxed_slice();
    g.maps.insert("map_d".to_string(), map_d);
    let mut geom_d = GGeometry::default();
    geom_d.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("map_d".to_string(), geom_d);

    prepare_map(&g, &"map_a".to_string());
    prepare_map(&g, &"map_b".to_string());
    prepare_map(&g, &"map_c".to_string());
    prepare_map(&g, &"map_d".to_string());

    // 1. Path from Map A to Map C successfully navigates through doors
    let path = get_path_nodes("map_a", 0.0, 0.0, "map_c", 50.0, 0.0, None);
    assert!(path.is_some(), "Expected cross-map path from map_a to map_c");
    let nodes = path.unwrap();
    assert!(!nodes.is_empty());
    assert_eq!(nodes.last().unwrap().map, "map_c");
    assert_eq!(nodes.last().unwrap().x, 50.0);

    // Verify door transitions are present in the path
    let door_nodes: Vec<_> = nodes.iter().filter(|n| n.method == "door").collect();
    assert!(
        door_nodes.len() >= 2,
        "Expected at least 2 door transitions (A->B, B->C)"
    );

    // 2. Disconnected map (map_a -> map_d) immediately returns None
    let unreachable = get_path_nodes("map_a", 0.0, 0.0, "map_d", 0.0, 0.0, None);
    assert_eq!(
        unreachable, None,
        "Disconnected map should return None via early exit"
    );

    clear();
}

#[test]
fn test_bank_penalty_routing() {
    let _lock = TEST_MUTEX.lock().unwrap();
    clear();

    let mut g = GData::default();

    // Map A connects to both "bank" and "map_b"
    let mut map_a = GMap::default();
    map_a.spawns = vec![[0.0, 0.0], [50.0, 0.0], [-50.0, 0.0]].into_boxed_slice();
    map_a.doors = vec![
        GDoor {
            x: 50.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            map_to: "bank".to_string(),
            spawn_to: 0,
            spawn_from: 1,
            requires_key: false,
            key: None,
        },
        GDoor {
            x: -50.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            map_to: "map_b".to_string(),
            spawn_to: 0,
            spawn_from: 2,
            requires_key: false,
            key: None,
        },
    ];
    g.maps.insert("map_a".to_string(), map_a);
    let mut geom_a = GGeometry::default();
    geom_a.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("map_a".to_string(), geom_a);

    // Bank connects to map_c
    let mut bank = GMap::default();
    bank.spawns = vec![[0.0, 0.0], [50.0, 0.0]].into_boxed_slice();
    bank.doors = vec![GDoor {
        x: 50.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        map_to: "map_c".to_string(),
        spawn_to: 0,
        spawn_from: 1,
        requires_key: false,
        key: None,
    }];
    g.maps.insert("bank".to_string(), bank);
    let mut geom_bank = GGeometry::default();
    geom_bank.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("bank".to_string(), geom_bank);

    // Map B connects to map_c
    let mut map_b = GMap::default();
    map_b.spawns = vec![[0.0, 0.0], [50.0, 0.0]].into_boxed_slice();
    map_b.doors = vec![GDoor {
        x: 50.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        map_to: "map_c".to_string(),
        spawn_to: 0,
        spawn_from: 1,
        requires_key: false,
        key: None,
    }];
    g.maps.insert("map_b".to_string(), map_b);
    let mut geom_b = GGeometry::default();
    geom_b.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("map_b".to_string(), geom_b);

    // Map C
    let mut map_c = GMap::default();
    map_c.spawns = vec![[0.0, 0.0]].into_boxed_slice();
    g.maps.insert("map_c".to_string(), map_c);
    let mut geom_c = GGeometry::default();
    geom_c.x_lines = Some(vec![[100, -50, 50]]);
    g.geometry.insert("map_c".to_string(), geom_c);

    prepare_map(&g, &"map_a".to_string());
    prepare_map(&g, &"bank".to_string());
    prepare_map(&g, &"map_b".to_string());
    prepare_map(&g, &"map_c".to_string());

    // Journey from map_a to map_c should route via map_b, avoiding the 120s bank penalty!
    let path = get_path_nodes("map_a", 0.0, 0.0, "map_c", 0.0, 0.0, None);
    assert!(path.is_some());
    let nodes = path.unwrap();
    let maps_visited: Vec<&str> = nodes.iter().map(|n| n.map.as_str()).collect();
    assert!(!maps_visited.contains(&"bank"), "Path should not go through bank!");
    assert!(maps_visited.contains(&"map_b"), "Path should route through map_b!");

    clear();
}

