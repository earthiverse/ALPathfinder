use bit_vec::BitVec;
use core::cmp::{max, min};
use once_cell::sync::Lazy;
use petgraph::graph::{Graph, NodeIndex};
use serde_wasm_bindgen::from_value;
use spade::{DelaunayTriangulation, FloatTriangulation, HasPosition, Point2, Triangulation};
use std::collections::HashMap;
use std::sync::RwLock;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use web_time::Instant;

mod g;
use crate::g::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

struct Grid {
    width: i32,
    min_x: i32,
    min_y: i32,
    data: BitVec,
}

const BASE_H: i32 = 8;
const BASE_V: i32 = 7;
const BASE_VN: i32 = 2;
const UNKNOWN: u8 = 0;
const NOT_WALKABLE: u8 = 1;
const WALKABLE: u8 = 2;

const TRANSPORT_RADIUS: u8 = 150;

#[derive(Debug, Clone)]
struct Node {
    map_id: u16,
    point: Point2<f32>,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        return self.map_id == other.map_id && self.point == other.point;
    }
}
impl Eq for Node {}
impl std::hash::Hash for Node {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.map_id.hash(state);
        self.point.x.to_bits().hash(state);
        self.point.y.to_bits().hash(state);
    }
}

impl HasPosition for Node {
    type Scalar = f32;

    fn position(&self) -> Point2<f32> {
        self.point
    }
}

const WALK: u8 = 1;
const TOWN: u8 = 2;
const DOOR: u8 = 4;
const TRANSPORT: u8 = 8;
const ENTER: u8 = 16;

struct Edge {
    method: u8,
}

const INSIDE_1: u8 = 0b0010_1111;
const INSIDE_1_IGNORE: u8 = 0b0010_0100;
const INSIDE_2: u8 = 0b1001_0111;
const INSIDE_2_IGNORE: u8 = 0b1000_0001;
const INSIDE_3: u8 = 0b1111_0100;
const INSIDE_3_IGNORE: u8 = 0b0010_0100;
const INSIDE_4: u8 = 0b1110_1001;
const INSIDE_4_IGNORE: u8 = 0b1000_0001;
const OUTSIDE_1: u8 = 0b0111_1111;
const OUTSIDE_2: u8 = 0b1101_1111;
const OUTSIDE_3: u8 = 0b1111_1011;
const OUTSIDE_4: u8 = 0b1111_1110;

static MAP_INDICES: Lazy<RwLock<HashMap<String, u16>>> = Lazy::new(|| RwLock::new(HashMap::new()));
static MAP_NAMES: Lazy<RwLock<Vec<String>>> = Lazy::new(|| RwLock::new(Vec::new()));

static GRAPH: Lazy<RwLock<Graph<Node, Edge>>> = Lazy::new(|| {
    let g = Graph::new();
    return RwLock::new(g);
});

static NODE_MAP: Lazy<RwLock<HashMap<Node, NodeIndex>>> = Lazy::new(|| {
    let m = HashMap::new();
    return RwLock::new(m);
});

static GRIDS: Lazy<RwLock<HashMap<String, Grid>>> = Lazy::new(|| {
    let m = HashMap::new();
    return RwLock::new(m);
});

fn get_or_add_node(node: &Node) -> NodeIndex {
    let mut node_map = NODE_MAP.write().unwrap();

    if let Some(&index) = node_map.get(&node) {
        index
    } else {
        let mut graph = GRAPH.write().unwrap();
        let index = graph.add_node(node.clone());
        node_map.insert(node.clone(), index);
        index
    }
}

fn get_or_create_map_id(map_name: &str) -> u16 {
    let indices = MAP_INDICES.read().unwrap();
    if let Some(&id) = indices.get(map_name) {
        return id;
    }
    drop(indices);

    let mut indices = MAP_INDICES.write().unwrap();
    let mut names = MAP_NAMES.write().unwrap();

    let id = names.len() as u16;
    names.push(map_name.to_string());
    indices.insert(map_name.to_string(), id);
    return id;
}

fn get_map_name(map_id: u32) -> Option<String> {
    let names = MAP_NAMES.read().unwrap();
    names.get(map_id as usize).cloned()
}

pub fn prepare_map(g: &GData, map_name: &String) {
    // Get the data
    let map = g.maps.get(map_name).unwrap();
    let geometry = g.geometry.get(map_name).unwrap();
    let width = geometry.max_x - geometry.min_x;
    let height = geometry.max_y - geometry.min_y;

    let walkable = prepare_walkable_vec(map, geometry, width, height);

    // Add the grid to the global list
    let mut grids = GRIDS.write().unwrap();
    grids.insert(
        map_name.to_string(),
        Grid {
            width,
            min_x: geometry.min_x,
            min_y: geometry.min_y,
            data: walkable.iter().map(|&state| state == WALKABLE).collect(),
        },
    );
    drop(grids);

    let map_id = get_or_create_map_id(map_name);
    let mut triangulation: DelaunayTriangulation<Node> = DelaunayTriangulation::new();

    // Add nodes at corners
    for y in 1..(height - 1) {
        let row_above = (y - 1) * width;
        let row_current = y * width;
        let row_below = (y + 1) * width;
        for x in 1..(width - 1) {
            let m_c = walkable[(row_current + x) as usize];
            if m_c != WALKABLE {
                continue;
            }

            let u_l = walkable[(row_above + x - 1) as usize];
            let u_c = walkable[(row_above + x) as usize];
            let u_r = walkable[(row_above + x + 1) as usize];
            let m_l = walkable[(row_current + x - 1) as usize];
            let m_r = walkable[(row_current + x + 1) as usize];
            let b_l = walkable[(row_below + x - 1) as usize];
            let b_c = walkable[(row_below + x) as usize];
            let b_r = walkable[(row_below + x + 1) as usize];

            let mask: u8 = (u_l == WALKABLE) as u8
                | ((u_c == WALKABLE) as u8) << 1
                | ((u_r == WALKABLE) as u8) << 2
                | ((m_l == WALKABLE) as u8) << 3
                | ((m_r == WALKABLE) as u8) << 4
                | ((b_l == WALKABLE) as u8) << 5
                | ((b_c == WALKABLE) as u8) << 6
                | ((b_r == WALKABLE) as u8) << 7;

            if (mask | INSIDE_1_IGNORE) == INSIDE_1
                || (mask | INSIDE_2_IGNORE) == INSIDE_2
                || (mask | INSIDE_3_IGNORE) == INSIDE_3
                || (mask | INSIDE_4_IGNORE) == INSIDE_4
                || mask == OUTSIDE_1
                || mask == OUTSIDE_2
                || mask == OUTSIDE_3
                || mask == OUTSIDE_4
            {
                let handle = triangulation.insert(Node {
                    map_id,
                    point: Point2::new((x + geometry.min_x) as f32, (y + geometry.min_y) as f32),
                });
                get_or_add_node(triangulation.vertex(handle.unwrap()).data());
            }
        }
    }

    // Add nodes at spawn points
    for spawn in &map.spawns {
        let handle = triangulation.insert(Node {
            map_id,
            point: Point2::new(spawn.x, spawn.y),
        });
        get_or_add_node(triangulation.vertex(handle.unwrap()).data());
    }

    // TODO: Add nodes for doors
    // TODO: Add door edges
    for door in &map.doors {
        // TODO: Make nodes at the four corners of the door
    }

    // Add nodes for transporters
    for npc in map.npcs.as_ref().unwrap() {
        if npc.id != "transporter" {
            continue;
        }

        // Make list of transporter destination nodes
        let mut destination_nodes: Vec<NodeIndex> = Vec::new();
        for (destination_map_name, &destination_spawn_index) in
            g.npcs.get("transporter").unwrap().places.as_ref().unwrap()
        {
            if destination_map_name == map_name {
                continue; // Can't transport to same map
            }
            let destination_map_id = get_or_create_map_id(destination_map_name);
            let destination_spawn = g
                .maps
                .get(destination_map_name)
                .unwrap()
                .spawns
                .get(destination_spawn_index as usize)
                .unwrap();

            let destination_node = Node {
                map_id: destination_map_id,
                point: Point2::new(destination_spawn.x, destination_spawn.y),
            };
            destination_nodes.push(get_or_add_node(&destination_node));
        }

        // Add transporter links to other maps
        if let Some(pos) = &npc.position {
            let x = pos[0];
            let y = pos[1];

            let _ = triangulation.insert(Node {
                map_id,
                point: Point2::new(x, y),
            });

            let nearby =
                triangulation.get_vertices_in_circle(Point2 { x, y }, TRANSPORT_RADIUS as f32);
            for n in nearby {
                let n_index = get_or_add_node(&n.data());
                for destination_node in &destination_nodes {
                    let mut graph = GRAPH.write().unwrap();
                    graph.add_edge(n_index, *destination_node, Edge { method: TRANSPORT });
                }
            }
        }
        if let Some(positions) = &npc.positions {
            for p in positions {
                let x = p[0];
                let y = p[1];

                let _ = triangulation.insert(Node {
                    map_id,
                    point: Point2::new(x, y),
                });

                let nearby =
                    triangulation.get_vertices_in_circle(Point2 { x, y }, TRANSPORT_RADIUS as f32);
                for n in nearby {
                    let n_index = get_or_add_node(&n.data());
                    for destination_node in &destination_nodes {
                        let mut graph = GRAPH.write().unwrap();
                        graph.add_edge(n_index, *destination_node, Edge { method: TRANSPORT });
                    }
                }
            }
        }
    }

    // Add all nodes to graph
    for edge in triangulation.undirected_edges() {
        let [p1, p2] = edge.vertices();
        let p1_data = p1.data();
        let p2_data = p2.data();

        if !can_walk_path(
            map_name,
            p1_data.point.x as i32,
            p1_data.point.y as i32,
            p2_data.point.x as i32,
            p2_data.point.y as i32,
        ) {
            continue;
        }

        // TODO: Calculate cost taking speed in to account when using A*
        // let cost = edge.length_2().sqrt();

        let p1_index = get_or_add_node(&p1_data);
        let p2_index = get_or_add_node(&p2_data);

        // Add the edges
        let mut graph = GRAPH.write().unwrap();
        graph.add_edge(p1_index, p2_index, Edge { method: WALK });
        graph.add_edge(p2_index, p1_index, Edge { method: WALK });
    }

    // TODO: Debug, remove
    let graph = GRAPH.read().unwrap();
    log(&format!(
        "{} processed. Graph now has {} nodes and {} edges.",
        map_name,
        graph.node_count(),
        graph.edge_count()
    ))
}

fn prepare_walkable_vec(map: &GMap, geometry: &GGeometry, width: i32, height: i32) -> Vec<u8> {
    let size: usize = (width * height) as usize;

    let mut walkable = vec![UNKNOWN; size];

    // Make the y-lines non-walkable
    match &geometry.y_lines {
        None => {}
        Some(v) => {
            for y_line in v {
                let y_from = max(0, y_line[0] - geometry.min_y - BASE_VN);
                let y_to = min(height, y_line[0] - geometry.min_y + BASE_V);
                for y in y_from..y_to {
                    let x_from = max(0, y_line[1] - geometry.min_x - BASE_H);
                    let x_to = min(width, y_line[2] - geometry.min_x + BASE_H);
                    for x in x_from..x_to {
                        walkable[(y * width + x) as usize] = NOT_WALKABLE;
                    }
                }
            }
        }
    }

    // Make the x-lines non-walkable
    match &geometry.x_lines {
        None => {}
        Some(v) => {
            for x_line in v {
                let x_from = max(0, x_line[0] - geometry.min_x - BASE_H);
                let x_to = min(width, x_line[0] - geometry.min_x + BASE_H);
                for x in x_from..x_to {
                    let y_from = max(0, x_line[1] - geometry.min_y - BASE_VN);
                    let y_to = min(height, x_line[2] - geometry.min_y + BASE_V);
                    for y in y_from..y_to {
                        walkable[(y * width + x) as usize] = NOT_WALKABLE;
                    }
                }
            }
        }
    }

    // Fill in the walkable areas
    for spawn in &map.spawns {
        let x = spawn.x.trunc() as i32 - geometry.min_x;
        let y = spawn.y.trunc() as i32 - geometry.min_y;

        if walkable[(y * width + x) as usize] == WALKABLE {
            // We've already determined this area is walkable
            continue;
        };

        let mut stack: Vec<(i32, i32)> = Vec::new();
        stack.push((y, x));
        while stack.len() > 0 {
            let (y, mut x) = stack.pop().unwrap();
            while x >= 0 && walkable[(y * width + x) as usize] == UNKNOWN {
                x -= 1;
            }
            x += 1;
            let mut span_above = false;
            let mut span_below = false;
            while x < width && walkable[(y * width + x) as usize] == UNKNOWN {
                walkable[(y * width + x) as usize] = WALKABLE;
                if !span_above && y > 0 && walkable[((y - 1) * width + x) as usize] == UNKNOWN {
                    stack.push((y - 1, x));
                    span_above = true;
                } else if span_above && y > 0 && walkable[((y - 1) * width + x) as usize] != UNKNOWN
                {
                    span_above = false;
                }

                if !span_below
                    && y < height - 1
                    && walkable[((y + 1) * width + x) as usize] == UNKNOWN
                {
                    stack.push((y + 1, x));
                    span_below = true;
                } else if span_below
                    && y < height - 1
                    && walkable[((y + 1) * width + x) as usize] != UNKNOWN
                {
                    span_below = false;
                }
                x += 1;
            }
        }
    }

    return walkable;
}

#[wasm_bindgen]
pub fn prepare(g_js: JsValue) {
    // Convert 'G' to a variable we can use
    let g: GData = from_value(g_js).unwrap();

    let start = Instant::now();
    for (map_name, map) in &g.maps {
        if map.ignore.is_some() {
            continue; // Skip ignored maps
        }

        // Make the grid
        prepare_map(&g, map_name);
    }

    // TODO: Debug, remove
    log(&format!(
        "Prepared all maps in {}ms!",
        start.elapsed().as_millis()
    ))
}

#[wasm_bindgen]
pub fn is_walkable(map_name: &str, x_i: i32, y_i: i32) -> bool {
    let grids = GRIDS.read().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return false,
    };

    // Convert the game coordinates to grid coordinates
    let x = x_i - grid.min_x;
    let y = y_i - grid.min_y;

    if x < 0 || y < 0 {
        return false;
    }

    return grid
        .data
        .get((y * grid.width + x) as usize)
        .unwrap_or(false);
}

#[wasm_bindgen]
pub fn can_walk_path(map_name: &str, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
    let grids = GRIDS.read().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return false,
    };

    let x_step: i32;
    let y_step: i32;
    let mut error: i32;
    let mut error_prev: i32;
    let mut x: i32 = x1 - grid.min_x;
    let mut y: i32 = y1 - grid.min_y;
    let mut dx: i32 = x2 - x1;
    let mut dy: i32 = y2 - y1;

    if !grid
        .data
        .get((y * grid.width + x) as usize)
        .unwrap_or(false)
    {
        return false;
    }

    if dy < 0 {
        y_step = -1;
        dy = -dy;
    } else {
        y_step = 1;
    }

    if dx < 0 {
        x_step = -1;
        dx = -dx;
    } else {
        x_step = 1;
    }
    let ddy = 2 * dy;
    let ddx = 2 * dx;

    if ddx >= ddy {
        error_prev = dx;
        error = dx;
        for _i in 0..dx {
            x += x_step;
            error += ddy;
            if error > ddx {
                y += y_step;
                error -= ddx;

                if error + error_prev < ddx {
                    if !grid
                        .data
                        .get(((y - y_step) * grid.width + x) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                } else if error + error_prev > ddx {
                    if !grid
                        .data
                        .get((y * grid.width + x - x_step) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                } else {
                    if !grid
                        .data
                        .get(((y - y_step) * grid.width + x) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                    if !grid
                        .data
                        .get((y * grid.width + x - x_step) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                }
            }
            if !grid
                .data
                .get((y * grid.width + x) as usize)
                .unwrap_or(false)
            {
                return false;
            }
            error_prev = error;
        }
    } else {
        error_prev = dy;
        error = dy;
        for _i in 0..dy {
            y += y_step;
            error += ddx;
            if error > ddy {
                x += x_step;
                error -= ddy;
                if error + error_prev < ddy {
                    if !grid
                        .data
                        .get((y * grid.width + x - x_step) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                } else if error + error_prev > ddy {
                    if !grid
                        .data
                        .get(((y - y_step) * grid.width + x) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                } else {
                    if !grid
                        .data
                        .get((y * grid.width + x - x_step) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                    if !grid
                        .data
                        .get(((y - y_step) * grid.width + x) as usize)
                        .unwrap_or(false)
                    {
                        return false;
                    }
                }
            }
            if !grid
                .data
                .get((y * grid.width + x) as usize)
                .unwrap_or(false)
            {
                return false;
            }
            error_prev = error;
        }
    }

    return true;
}
