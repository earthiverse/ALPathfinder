use alpathfinder::g::{GData, GGeometry, GMap};
use alpathfinder::{
    add_cheat_path, clear, get_safe_walk_node, is_walkable, prepare_map, SafeWalkPoint,
};
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_safe_walk_suite() {
    let _lock = TEST_MUTEX.lock().unwrap();

    // 1. Map with a wall at x = 50
    clear();
    let mut g = GData::default();
    let mut map_data = GMap::default();
    map_data.spawns = vec![[0.0, 0.0]].into_boxed_slice();
    g.maps.insert("main".to_string(), map_data);

    let mut geom_data = GGeometry::default();
    // Wall at x = 50, from y = -100 to 100 (unwalkable x: [41, 59], y: [-103, 108])
    geom_data.x_lines = Some(vec![[50, -100, 100]]);
    g.geometry.insert("main".to_string(), geom_data);

    prepare_map(&g, &"main".to_string());

    assert!(is_walkable("main", 0.0, 0.0));
    assert!(is_walkable("main", 30.0, 0.0));
    assert!(!is_walkable("main", 50.0, 0.0));

    // A. Unobstructed walk to a point before the wall
    let result_unobstructed = get_safe_walk_node("main", 0.0, 0.0, 30.0, 0.0);
    assert_eq!(
        result_unobstructed,
        Some(SafeWalkPoint {
            map: "main".to_string(),
            x: 30.0,
            y: 0.0,
        })
    );

    // B. Walk towards destination beyond the wall at (100, 0)
    // Should stop right in front of the wall boundary (x <= 41.0)
    let result_blocked = get_safe_walk_node("main", 0.0, 0.0, 100.0, 0.0);
    assert!(result_blocked.is_some());
    let pt = result_blocked.unwrap();
    assert_eq!(pt.map, "main");
    assert_eq!(pt.y, 0.0);
    assert!(pt.x <= 41.0);
    assert!(pt.x > 38.0);
    assert!(is_walkable("main", pt.x, pt.y));

    // C. Starting inside an unwalkable wall returns None
    let result_inside = get_safe_walk_node("main", 50.0, 0.0, 100.0, 0.0);
    assert_eq!(result_inside, None);

    // D. Same start and destination point returns the start point
    let result_same = get_safe_walk_node("main", 10.0, 0.0, 10.0, 0.0);
    assert_eq!(
        result_same,
        Some(SafeWalkPoint {
            map: "main".to_string(),
            x: 10.0,
            y: 0.0,
        })
    );

    // 2. With cheat path across the wall
    clear();
    let mut g = GData::default();
    let mut map_data = GMap::default();
    map_data.spawns = vec![[0.0, 0.0], [100.0, 0.0]].into_boxed_slice();
    g.maps.insert("main".to_string(), map_data);

    let mut geom_data = GGeometry::default();
    geom_data.x_lines = Some(vec![[50, -100, 100]]);
    g.geometry.insert("main".to_string(), geom_data);

    add_cheat_path("main", 0.0, 0.0, 100.0, 0.0);
    prepare_map(&g, &"main".to_string());

    let result_cheat = get_safe_walk_node("main", 0.0, 0.0, 100.0, 0.0);
    assert_eq!(
        result_cheat,
        Some(SafeWalkPoint {
            map: "main".to_string(),
            x: 100.0,
            y: 0.0,
        })
    );

    clear();
}
