use alpathfinder::g::{GData, GGeometry, GMap};
use alpathfinder::{clear, get_path_nodes, prepare_map};
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_town_warp_vs_walking() {
    let _lock = TEST_MUTEX.lock().unwrap();
    clear();

    let mut g = GData::default();

    let mut map = GMap::default();
    // Spawn 0 is town spawn at (0, 0), Spawn 1 is far away at (1000, 1000)
    map.spawns = vec![[0.0, 0.0], [1000.0, 1000.0]].into_boxed_slice();
    g.maps.insert("town_map".to_string(), map);

    let mut geom = GGeometry::default();
    geom.x_lines = Some(vec![[2000, -2000, 2000]]);
    g.geometry.insert("town_map".to_string(), geom);

    prepare_map(&g, &"town_map".to_string()).expect("Failed to prepare town_map");

    // Case 1: Start far away at (1000, 1000), target near town spawn (5.0, 0.0).
    // Walking distance is > 1400 units (>17s at speed 80).
    // Town warp takes 3.812s + short walk (0.06s), so town warp should be chosen!
    let path = get_path_nodes("town_map", 1000.0, 1000.0, "town_map", 5.0, 0.0, Some(80.0), &[])
        .expect("Path should be found");

    assert!(path.len() >= 2, "Path must contain at least 2 nodes");
    let has_town_step = path.iter().any(|node| node.method == "town");
    assert!(
        has_town_step,
        "Path should utilize town warp when walking takes significantly longer than 3.812s"
    );

    // Case 2: Start near town spawn (0.0, 0.0), target (20.0, 0.0).
    // Walking distance is 20 units (0.25s at speed 80).
    // Walking directly is much faster than town warp (3.812s).
    let short_path = get_path_nodes("town_map", 0.0, 0.0, "town_map", 20.0, 0.0, Some(80.0), &[])
        .expect("Short path should be found");

    assert!(
        short_path.iter().all(|node| node.method == "move"),
        "Short walk should not use town warp"
    );
}
