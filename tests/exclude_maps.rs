use alpathfinder::g::{GData, GDoor, GGeometry, GMap};
use alpathfinder::{clear, get_path_nodes, prepare_map};
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_exclude_maps_reroutes_around_shorter_path() {
    let _lock = TEST_MUTEX.lock().unwrap();
    clear();

    let mut g = GData::default();

    let mut start_map = GMap::default();
    start_map.spawns = vec![[0.0, 0.0]].into_boxed_slice();
    start_map.doors = vec![
        GDoor {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            map_to: "long_map".to_string(),
            spawn_to: 0,
            spawn_from: 0,
            requires_key: false,
            key: None,
        },
        GDoor {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            map_to: "short_map".to_string(),
            spawn_to: 0,
            spawn_from: 0,
            requires_key: false,
            key: None,
        },
    ];
    g.maps.insert("start_map".to_string(), start_map);
    g.geometry
        .insert("start_map".to_string(), GGeometry::default());

    // Long walk: 1000 units
    let mut long_map = GMap::default();
    long_map.spawns = vec![[0.0, 0.0], [1000.0, 0.0]].into_boxed_slice();
    long_map.doors = vec![GDoor {
        x: 1000.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        map_to: "finish_map".to_string(),
        spawn_to: 0,
        spawn_from: 1,
        requires_key: false,
        key: None,
    }];
    g.maps.insert("long_map".to_string(), long_map);
    g.geometry
        .insert("long_map".to_string(), GGeometry::default());

    // Short walk: 20 units
    let mut short_map = GMap::default();
    short_map.spawns = vec![[0.0, 0.0], [20.0, 0.0]].into_boxed_slice();
    short_map.doors = vec![GDoor {
        x: 20.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        map_to: "finish_map".to_string(),
        spawn_to: 0,
        spawn_from: 1,
        requires_key: false,
        key: None,
    }];
    g.maps.insert("short_map".to_string(), short_map);
    g.geometry
        .insert("short_map".to_string(), GGeometry::default());

    let mut finish_map = GMap::default();
    finish_map.spawns = vec![[0.0, 0.0]].into_boxed_slice();
    g.maps.insert("finish_map".to_string(), finish_map);
    g.geometry
        .insert("finish_map".to_string(), GGeometry::default());

    prepare_map(&g, &"start_map".to_string()).expect("Failed to prepare start_map");
    prepare_map(&g, &"long_map".to_string()).expect("Failed to prepare long_map");
    prepare_map(&g, &"short_map".to_string()).expect("Failed to prepare short_map");
    prepare_map(&g, &"finish_map".to_string()).expect("Failed to prepare finish_map");

    let unrestricted_path = get_path_nodes(
        "start_map",
        0.0,
        0.0,
        "finish_map",
        0.0,
        0.0,
        Some(80.0),
        &[],
    )
    .expect("Path should be found");

    let unrestricted_maps_visited: Vec<&str> =
        unrestricted_path.iter().map(|n| n.map.as_str()).collect();
    assert!(
        unrestricted_maps_visited.contains(&"short_map"),
        "Path should route through short_map when nothing is excluded"
    );
    assert!(
        !unrestricted_maps_visited.contains(&"long_map"),
        "Path should not route through long_map when short_map is available"
    );
    assert_eq!(unrestricted_path.last().unwrap().map, "finish_map");

    let restricted_path = get_path_nodes(
        "start_map",
        0.0,
        0.0,
        "finish_map",
        0.0,
        0.0,
        Some(80.0),
        &["short_map".to_string()],
    )
    .expect("Path should still be found when routing around short_map");

    let restricted_maps_visited: Vec<&str> =
        restricted_path.iter().map(|n| n.map.as_str()).collect();
    assert!(
        restricted_maps_visited.contains(&"long_map"),
        "Path should route through long_map when short_map is excluded"
    );
    assert!(
        !restricted_maps_visited.contains(&"short_map"),
        "Path should not route through excluded short_map"
    );
    assert_eq!(restricted_path.last().unwrap().map, "finish_map");

    clear();
}
