use alpathfinder::g::{GData, GGeometry, GMap};
use alpathfinder::{
    add_cheat_path, can_walk_path, clear, get_path_nodes, is_walkable, prepare_map,
};

#[test]
fn test_add_cheat_path() {
    // 1. Without cheat path
    clear();
    let mut g = GData::default();
    let mut map_data = GMap::default();
    map_data.spawns = vec![[0.0, 0.0]].into_boxed_slice();
    g.maps.insert("main".to_string(), map_data);

    let mut geom_data = GGeometry::default();
    // Wall at x = 50, from y = -100 to 100
    geom_data.x_lines = Some(vec![[50, -100, 100]]);
    g.geometry.insert("main".to_string(), geom_data);

    prepare_map(&g, &"main".to_string());
    assert!(!can_walk_path("main", 0, 0, 100, 0));
    assert!(!is_walkable("main", 50.0, 0.0));

    // 2. With cheat path
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

    assert!(can_walk_path("main", 0, 0, 100, 0));
    // Off by 1 pixel is NOT walkable
    assert!(!can_walk_path("main", 0, 1, 100, 1));

    let path = get_path_nodes("main", 0.0, 0.0, "main", 100.0, 0.0, None);
    assert!(path.is_some());
    let nodes = path.unwrap();
    assert!(!nodes.is_empty());
    assert_eq!(nodes.last().unwrap().x, 100.0);
    assert_eq!(nodes.last().unwrap().y, 0.0);

    // 3. Cheat path to an unwalkable point inside a wall is rejected
    clear();
    let mut g = GData::default();
    let mut map_data = GMap::default();
    map_data.spawns = vec![[0.0, 0.0]].into_boxed_slice();
    g.maps.insert("main".to_string(), map_data);

    let mut geom_data = GGeometry::default();
    geom_data.x_lines = Some(vec![[50, -100, 100]]);
    g.geometry.insert("main".to_string(), geom_data);

    // (50, 0) is inside the wall (unwalkable)
    add_cheat_path("main", 0.0, 0.0, 50.0, 0.0);
    prepare_map(&g, &"main".to_string());

    assert!(!can_walk_path("main", 0, 0, 50, 0));

    clear();
}
