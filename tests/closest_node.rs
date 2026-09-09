use alpathfinder::g::{GData, GGeometry, GMap};
use alpathfinder::{clear, get_closest_node, prepare_map};
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_find_closest_node_partitioning_and_walls() {
    let _lock = TEST_MUTEX.lock().unwrap();
    clear();

    let mut g = GData::default();

    let mut main_map = GMap::default();
    main_map.spawns = vec![[0.0, 0.0], [100.0, 0.0]].into_boxed_slice();
    g.maps.insert("main".to_string(), main_map);

    let mut hw_map = GMap::default();
    hw_map.spawns = vec![[500.0, 500.0]].into_boxed_slice();
    g.maps.insert("halloween".to_string(), hw_map);

    let mut geom = GGeometry::default();
    // Wall at x = 50, separating (0, 0) from (100, 0)
    geom.x_lines = Some(vec![[50, -100, 100]]);
    g.geometry.insert("main".to_string(), geom);

    let mut hw_geom = GGeometry::default();
    hw_geom.x_lines = Some(vec![[600, 400, 600]]);
    g.geometry.insert("halloween".to_string(), hw_geom);

    prepare_map(&g, &"main".to_string());
    prepare_map(&g, &"halloween".to_string());

    // Querying "halloween" finds a reachable node on "halloween"
    let hw_node = get_closest_node("halloween", 500.0, 500.0);
    assert!(hw_node.is_some());

    // At (10.0, 0.0), node at (0.0, 0.0) is reachable and closest
    let main_node = get_closest_node("main", 10.0, 0.0);
    assert!(main_node.is_some());

    // Unknown map returns None
    assert_eq!(get_closest_node("nonexistent", 0.0, 0.0), None);

    clear();
}
