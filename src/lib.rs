use clipper2_rust::{ClipType, Clipper64, FillRule, PathType, Paths64, Point};
use once_cell::sync::Lazy;
use petgraph::algo::astar;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::from_value;
use spade::handles::{FixedDirectedEdgeHandle, FixedVertexHandle};
use spade::{
    ConstrainedDelaunayTriangulation, FloatTriangulation, HasPosition, Point2, Triangulation,
};
use std::collections::{HashMap, VecDeque};
use std::mem;
use std::sync::RwLock;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

pub mod g;
pub use crate::g::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
import type { GData, ItemKey, MapKey } from "typed-adventureland";

export interface PathNode {
  map: MapKey;
  x: number;
  y: number;
  method: "move" | "town" | "door" | "transport" | "enter" | "leave" | "unknown";
  spawn?: number;
  key?: ItemKey;
}

export interface SafeWalkPoint {
  map: MapKey;
  x: number;
  y: number;
}

export function addCheatPath(
  map: MapKey,
  x_from: number,
  y_from: number,
  x_to: number,
  y_to: number,
): void;

export function canWalkPath(
  map: MapKey,
  x_from: number,
  y_from: number,
  x_to: number,
  y_to: number,
): boolean;

export function getPath(
  map_from: MapKey,
  x_from: number,
  y_from: number,
  map_to: MapKey,
  x_to: number,
  y_to: number,
  /** How fast your character can walk. Setting this to a very high number can also reduce the number of town warps. */
  speed?: number,
): PathNode[] | null;

export function getSafeWalkTo(
  map: MapKey,
  x_from: number,
  y_from: number,
  x_to: number,
  y_to: number,
): SafeWalkPoint | null;

export function isWalkable(map: MapKey, x: number, y: number): boolean;

export function prepare(g: GData, exclude_maps?: MapKey[]): void;"#;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SafeWalkPoint {
    pub map: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PathNode {
    pub map: String,
    pub x: f32,
    pub y: f32,
    pub method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spawn: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

type CDT = ConstrainedDelaunayTriangulation<VertexData, (), EdgeData>;

#[derive(Debug, Clone, Default)]
struct EdgeData {
    reachable: bool,
}

pub struct VertexData {
    pub point: Point2<f32>,
    pub node_index: NodeIndex,
}

const BASE_H: i32 = 8;
const BASE_V: i32 = 7;
const BASE_VN: i32 = 2;

const DOOR_RADIUS: f32 = 40.0;
const TRANSPORT_RADIUS_SQUARED: f32 = 22500.0; // 150^2
const BANK_PENALTY: f32 = 120.0; // Deincentivize moving through bank

#[derive(Debug, Clone, PartialEq)]
struct Node {
    map_id: usize,
    point: Point2<f32>,
}

impl Eq for Node {}

impl std::hash::Hash for Node {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.map_id.hash(state);
        self.point.x.to_bits().hash(state);
        self.point.y.to_bits().hash(state);
    }
}

impl HasPosition for VertexData {
    type Scalar = f32;
    fn position(&self) -> Point2<Self::Scalar> {
        self.point
    }
}

#[derive(Clone)]
enum Edge {
    WALK(f32),
    TOWN,
    DOOR(u8),
    TRANSPORT(u8),
    ENTER(Box<(u8, Option<String>)>),
    LEAVE(u8),
}

#[derive(Debug, Clone)]
struct CheatPath {
    map: String,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

static CHEAT_PATHS: Lazy<RwLock<Vec<CheatPath>>> = Lazy::new(|| RwLock::new(Vec::new()));

#[wasm_bindgen(js_name = addCheatPath, skip_typescript)]
pub fn add_cheat_path(map: &str, x1: f32, y1: f32, x2: f32, y2: f32) {
    let mut paths = CHEAT_PATHS.write().ok().unwrap();
    paths.push(CheatPath {
        map: map.to_string(),
        x1,
        y1,
        x2,
        y2,
    });
}

#[derive(Clone, Debug)]
struct MapExit {
    point: Point2<f32>,
    target_map_id: usize,
    cost: f32,
}

static MAP_NAMES: RwLock<Vec<String>> = RwLock::new(Vec::new());

static GRAPH: Lazy<RwLock<Graph<Node, Edge>>> = Lazy::new(|| RwLock::new(Graph::new()));

static MAP_INDICES: RwLock<FxHashMap<String, usize>> =
    RwLock::new(HashMap::with_hasher(FxBuildHasher));

static NODE_MAP: RwLock<FxHashMap<Node, NodeIndex>> =
    RwLock::new(HashMap::with_hasher(FxBuildHasher));

static MAP_NODES: RwLock<Vec<Vec<NodeIndex>>> = RwLock::new(Vec::new());

static MAP_EXITS: RwLock<Vec<Vec<MapExit>>> = RwLock::new(Vec::new());

static MAP_DISTANCES: RwLock<Vec<f32>> = RwLock::new(Vec::new());

static TOPOLOGY_DIRTY: RwLock<bool> = RwLock::new(true);

static GRIDS: RwLock<FxHashMap<String, CDT>> = RwLock::new(HashMap::with_hasher(FxBuildHasher));

#[wasm_bindgen(skip_typescript)]
pub fn clear() {
    *MAP_NAMES.write().ok().unwrap() = Vec::new();
    *GRAPH.write().ok().unwrap() = Graph::new();
    *MAP_INDICES.write().ok().unwrap() = HashMap::with_hasher(FxBuildHasher);
    *NODE_MAP.write().ok().unwrap() = HashMap::with_hasher(FxBuildHasher);
    *MAP_NODES.write().ok().unwrap() = Vec::new();
    *MAP_EXITS.write().ok().unwrap() = Vec::new();
    *MAP_DISTANCES.write().ok().unwrap() = Vec::new();
    *TOPOLOGY_DIRTY.write().ok().unwrap() = true;
    *GRIDS.write().ok().unwrap() = HashMap::with_hasher(FxBuildHasher);
    *CHEAT_PATHS.write().ok().unwrap() = Vec::new();
}

fn add_map_exit(from_map_id: usize, exit: MapExit) {
    let mut exits = MAP_EXITS.write().ok().unwrap();
    if exits.len() <= from_map_id {
        exits.resize(from_map_id + 1, Vec::new());
    }
    exits[from_map_id].push(exit);
    *TOPOLOGY_DIRTY.write().ok().unwrap() = true;
}

fn ensure_map_topology() {
    if !*TOPOLOGY_DIRTY.read().ok().unwrap() {
        return;
    }
    let mut dirty = TOPOLOGY_DIRTY.write().ok().unwrap();
    if !*dirty {
        return;
    }

    let map_count = MAP_NAMES.read().ok().unwrap().len();
    if map_count == 0 {
        *dirty = false;
        return;
    }

    let mut dist = vec![f32::INFINITY; map_count * map_count];
    for i in 0..map_count {
        dist[i * map_count + i] = 0.0;
    }

    let exits = MAP_EXITS.read().ok().unwrap();
    for (from_map, map_exit_list) in exits.iter().enumerate() {
        if from_map >= map_count {
            continue;
        }
        for exit in map_exit_list {
            if exit.target_map_id < map_count {
                let idx = from_map * map_count + exit.target_map_id;
                if exit.cost < dist[idx] {
                    dist[idx] = exit.cost;
                }
            }
        }
    }

    for k in 0..map_count {
        for i in 0..map_count {
            let ik = dist[i * map_count + k];
            if ik.is_infinite() {
                continue;
            }
            for j in 0..map_count {
                let kj = dist[k * map_count + j];
                if kj.is_infinite() {
                    continue;
                }
                let alt = ik + kj;
                let ij = &mut dist[i * map_count + j];
                if alt < *ij {
                    *ij = alt;
                }
            }
        }
    }

    *MAP_DISTANCES.write().ok().unwrap() = dist;
    *dirty = false;
}

fn get_or_add_node(node: Node) -> NodeIndex {
    let mut node_map = NODE_MAP.write().ok().unwrap();
    *node_map
        .entry(node.clone())
        .or_insert_with(|| {
            let map_id = node.map_id;
            let node_index = GRAPH.write().ok().unwrap().add_node(node);
            let mut map_nodes = MAP_NODES.write().ok().unwrap();
            if map_nodes.len() <= map_id {
                map_nodes.resize(map_id + 1, Vec::new());
            }
            map_nodes[map_id].push(node_index);
            node_index
        })
}

fn get_or_create_map_id(map_name: &str) -> usize {
    let mut indices = MAP_INDICES.write().ok().unwrap();
    if let Some(id) = indices.get(map_name) {
        return *id;
    }
    let mut names = MAP_NAMES.write().ok().unwrap();

    let id = names.len();
    let as_string = map_name.to_string();
    names.push(as_string.clone());
    indices.insert(as_string, id);
    return id;
}

fn get_map_name(map_id: usize) -> Option<String> {
    let names = MAP_NAMES.read().ok().unwrap();
    names.get(map_id).cloned()
}

pub fn get_closest_node(map_name: &str, x: f32, y: f32) -> Option<NodeIndex> {
    let graph = GRAPH.read().ok()?;
    find_closest_node(&graph, map_name, x, y)
}

pub(crate) fn find_closest_node(
    graph: &Graph<Node, Edge>,
    map_name: &str,
    x: f32,
    y: f32,
) -> Option<NodeIndex> {
    let target_map_id = *MAP_INDICES.read().ok().unwrap().get(map_name)?;
    let map_nodes = MAP_NODES.read().ok().unwrap();
    let nodes_for_map = map_nodes.get(target_map_id)?;

    if nodes_for_map.is_empty() {
        return None;
    }

    let mut candidates: Vec<(NodeIndex, f32)> = nodes_for_map
        .iter()
        .map(|&idx| {
            let node = &graph[idx];
            let dx = node.point.x - x;
            let dy = node.point.y - y;
            (idx, dx * dx + dy * dy)
        })
        .collect();

    candidates.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    for (node_index, _dist) in candidates {
        let node = &graph[node_index];
        if can_walk_path(
            map_name,
            node.point.x.trunc() as i32,
            node.point.y.trunc() as i32,
            x.trunc() as i32,
            y.trunc() as i32,
        ) {
            return Some(node_index);
        }
    }

    None
}

fn flood_fill_holes(triangulation: &mut CDT, spawn_points: Vec<FixedVertexHandle>) {
    let mut queue: VecDeque<FixedDirectedEdgeHandle> = VecDeque::new();

    for vertex in spawn_points {
        for edge in triangulation.vertex(vertex).out_edges() {
            queue.push_back(edge.fix());
        }
    }

    while let Some(edge) = queue.pop_front() {
        let edge = triangulation.directed_edge(edge);
        if !edge.is_outer_edge() {
            for edge in [edge.next().fix(), edge.prev().fix()] {
                let data = &mut triangulation.undirected_edge_data_mut(edge.as_undirected());
                if mem::replace(&mut data.data_mut().reachable, true) || data.is_constraint_edge() {
                    continue;
                }

                queue.push_back(edge.rev());
            }
        }
    }
}

pub fn prepare_map(g: &GData, map_name: &String) -> Option<()> {
    // Get the data
    let map = g.maps.get(map_name)?;
    let geometry = g.geometry.get(map_name)?;

    let map_id = get_or_create_map_id(map_name);

    let mut clip = Clipper64::new();

    if let Some(v) = &geometry.y_lines {
        for &[y, x0, x1] in v {
            let y_from = (y - BASE_VN - 1) as i64;
            let y_to = (y + BASE_V + 1) as i64;
            let x_from = (x0 - BASE_H - 1) as i64;
            let x_to = (x1 + BASE_H + 1) as i64;
            clip.base.add_path(
                &vec![
                    Point::new(x_from, y_from),
                    Point::new(x_to, y_from),
                    Point::new(x_to, y_to),
                    Point::new(x_from, y_to),
                ],
                PathType::Subject,
                false,
            );
        }
    }

    if let Some(v) = &geometry.x_lines {
        for &[x, y0, y1] in v {
            let x_from = (x - BASE_H - 1) as i64;
            let x_to = (x + BASE_H + 1) as i64;
            let y_from = (y0 - BASE_VN - 1) as i64;
            let y_to = (y1 + BASE_V + 1) as i64;
            clip.base.add_path(
                &vec![
                    Point::new(x_from, y_from),
                    Point::new(x_to, y_from),
                    Point::new(x_to, y_to),
                    Point::new(x_from, y_to),
                ],
                PathType::Subject,
                false,
            );
        }
    }

    let mut sol = Paths64::new();

    clip.execute(ClipType::Union, FillRule::NonZero, &mut sol, None);

    let mut triangulation: CDT = ConstrainedDelaunayTriangulation::new();

    for path in sol.into_iter() {
        let _ = triangulation.add_constraint_edges(
            path.into_iter().map(|point| {
                let point = Point2::new(point.x as f32, point.y as f32);
                let node_index = get_or_add_node(Node {
                    map_id,
                    point: point.clone(),
                });
                VertexData { point, node_index }
            }),
            true,
        );
    }

    // Add nodes at spawn points
    let spawns: Vec<FixedVertexHandle> = map
        .spawns
        .iter()
        .map(|&[x, y]| {
            let point = Point2 { x, y };
            let node_index = get_or_add_node(Node {
                map_id,
                point: point.clone(),
            });
            triangulation
                .insert(VertexData { point, node_index })
                .ok()
                .unwrap()
        })
        .collect();

    for door in map.doors.iter() {
        let &[spawn_x, spawn_y] = map.spawns.get(door.spawn_from as usize)?;
        let half_w = door.width * 0.5;
        let half_h = door.height * 0.5;

        let points = [
            // Bottom middle
            Point2::new(spawn_x, spawn_y),
            // Corners
            Point2::new(spawn_x - half_w, spawn_y),
            Point2::new(spawn_x + half_w, spawn_y),
            Point2::new(spawn_x + half_w, spawn_y - door.height),
            Point2::new(spawn_x - half_w, spawn_y - door.height),
            // Mid points
            Point2::new(spawn_x - half_w, spawn_y - half_h),
            Point2::new(spawn_x + half_w, spawn_y - half_h),
            Point2::new(spawn_x, spawn_y - door.height),
        ];

        let destination_map_id = get_or_create_map_id(&door.map_to);
        let &[dsx, dsy] = g
            .maps
            .get(&door.map_to)?
            .spawns
            .get(door.spawn_to as usize)?;

        let destination_node_index = get_or_add_node(Node {
            map_id: destination_map_id,
            point: Point2::new(dsx, dsy),
        });

        let mut exit_cost = 0.812;
        if !map_name.starts_with("bank") && door.map_to.starts_with("bank") {
            exit_cost += BANK_PENALTY;
        }
        add_map_exit(
            map_id,
            MapExit {
                point: Point2::new(spawn_x, spawn_y),
                target_map_id: destination_map_id,
                cost: exit_cost,
            },
        );

        // Add nodes and edges for doors
        let nearby = triangulation.get_vertices_in_rectangle(
            Point2::new(
                spawn_x - half_w - DOOR_RADIUS,
                door.y - door.height - DOOR_RADIUS,
            ),
            Point2::new(spawn_x + half_w + DOOR_RADIUS, door.y + DOOR_RADIUS),
        );
        let edge = if door.requires_key {
            Edge::ENTER(Box::new((door.spawn_to, door.key.clone())))
        } else {
            Edge::DOOR(door.spawn_to)
        };
        for n in nearby {
            let n_index = n.data().node_index;

            GRAPH
                .write()
                .ok()
                .unwrap()
                .add_edge(n_index, destination_node_index, edge.clone());
        }

        for point in points {
            let index = get_or_add_node(Node { map_id, point });
            let _ = triangulation.insert(VertexData {
                point,
                node_index: index.clone(),
            });
            GRAPH
                .write()
                .ok()
                .unwrap()
                .add_edge(index, destination_node_index, edge.clone());
        }
    }

    // Add nodes for transporters
    if let Some(npcs) = &map.npcs {
        for npc in npcs.iter() {
            if npc.id != "transporter" {
                continue;
            }

            // Make list of transporter destination nodes
            let mut destination_nodes: Vec<(NodeIndex, u8)> = Vec::new();
            for (destination_map_name, destination_spawn_index) in
                g.npcs.get("transporter")?.places.as_ref().unwrap()
            {
                let destination_map_id = get_or_create_map_id(destination_map_name);
                if destination_map_id == map_id {
                    continue; // Can't transport to same map
                }
                let &[dsx, dsy] = g
                    .maps
                    .get(destination_map_name)?
                    .spawns
                    .get(*destination_spawn_index as usize)?;

                destination_nodes.push((
                    get_or_add_node(Node {
                        map_id: destination_map_id,
                        point: Point2::new(dsx, dsy),
                    }),
                    *destination_spawn_index,
                ));
            }

            // Add transporter links to other maps
            if let &Some([x, y]) = &npc.position {
                let point = Point2 { x, y };
                let node_index = get_or_add_node(Node {
                    map_id,
                    point: point.clone(),
                });
                let _ = triangulation.insert(VertexData {
                    point: point.clone(),
                    node_index,
                });

                for (dest_map_name, _) in g.npcs.get("transporter")?.places.as_ref().unwrap() {
                    let dest_id = get_or_create_map_id(dest_map_name);
                    if dest_id != map_id {
                        add_map_exit(
                            map_id,
                            MapExit {
                                point,
                                target_map_id: dest_id,
                                cost: 3.2,
                            },
                        );
                    }
                }

                for n in triangulation.get_vertices_in_circle(point, TRANSPORT_RADIUS_SQUARED) {
                    let n_index = n.data().node_index;
                    for (destination_node, spawn) in &destination_nodes {
                        GRAPH.write().ok().unwrap().add_edge(
                            n_index,
                            *destination_node,
                            Edge::TRANSPORT(*spawn),
                        );
                    }
                }
            }
            if let Some(positions) = &npc.positions {
                for &[x, y] in positions {
                    let point = Point2 { x, y };
                    let node_index = get_or_add_node(Node {
                        map_id,
                        point: point.clone(),
                    });
                    let _ = triangulation.insert(VertexData {
                        point: point.clone(),
                        node_index,
                    });

                    for (dest_map_name, _) in g.npcs.get("transporter")?.places.as_ref().unwrap() {
                        let dest_id = get_or_create_map_id(dest_map_name);
                        if dest_id != map_id {
                            add_map_exit(
                                map_id,
                                MapExit {
                                    point,
                                    target_map_id: dest_id,
                                    cost: 3.2,
                                },
                            );
                        }
                    }

                    for n in triangulation.get_vertices_in_circle(point, TRANSPORT_RADIUS_SQUARED) {
                        let n_index = n.data().node_index;
                        for (destination_node, spawn) in &destination_nodes {
                            GRAPH.write().ok().unwrap().add_edge(
                                n_index,
                                *destination_node,
                                Edge::TRANSPORT(*spawn),
                            );
                        }
                    }
                }
            }
        }
    }

    // Add leave logic (leaving sends you to spawn 0 on main)
    if map_name == "jail" || map_name == "cyberland" {
        let destination_map_id = get_or_create_map_id("main");
        let &[dsx, dsy] = g.maps.get("main")?.spawns.get(0)?;
        let destination_node_index = get_or_add_node(Node {
            map_id: destination_map_id,
            point: Point2::new(dsx, dsy),
        });
        add_map_exit(
            map_id,
            MapExit {
                point: Point2::new(0.0, 0.0),
                target_map_id: destination_map_id,
                cost: 0.812,
            },
        );
        let mut graph = GRAPH.write().ok().unwrap();
        for vertice in triangulation.vertices() {
            let node_index = vertice.data().node_index;
            graph.add_edge(node_index, destination_node_index, Edge::LEAVE(0));
        }
    }

    // Add edges to town spawn
    let town_spawn_index = triangulation.vertex(*spawns.get(0)?).data().node_index;

    flood_fill_holes(&mut triangulation, spawns);

    GRIDS
        .write()
        .ok()
        .unwrap()
        .insert(map_name.to_string(), triangulation);

    // Pre-calculate cheat path nodes for walkable endpoints before locking graph
    let cheat_paths = CHEAT_PATHS.read().ok().unwrap();
    let cheat_edges: Vec<(NodeIndex, NodeIndex)> = cheat_paths
        .iter()
        .filter(|cp| {
            cp.map == *map_name
                && is_walkable(map_name, cp.x1, cp.y1)
                && is_walkable(map_name, cp.x2, cp.y2)
        })
        .map(|cp| {
            let node_a = get_or_add_node(Node {
                map_id,
                point: Point2::new(cp.x1, cp.y1),
            });
            let node_b = get_or_add_node(Node {
                map_id,
                point: Point2::new(cp.x2, cp.y2),
            });
            (node_a, node_b)
        })
        .collect();

    let mut graph = GRAPH.write().ok().unwrap();
    for vertice in GRIDS.read().ok().unwrap().get(map_name).unwrap().vertices() {
        let node_index = vertice.data().node_index;
        if node_index == town_spawn_index {
            continue;
        }

        graph.add_edge(node_index, town_spawn_index, Edge::TOWN);
    }

    // Add all nodes to graph
    for edge in GRIDS
        .read()
        .ok()
        .unwrap()
        .get(map_name)
        .unwrap()
        .undirected_edges()
    {
        if !edge.data().data().reachable {
            continue;
        }

        let [p1, p2] = edge.vertices();

        let p1_index = p1.data().node_index;
        let p2_index = p2.data().node_index;

        let pos1 = p1.position();
        let pos2 = p2.position();
        let dx = pos2.x - pos1.x;
        let dy = pos2.y - pos1.y;
        let dist = (dx * dx + dy * dy).sqrt();

        graph.add_edge(p1_index, p2_index, Edge::WALK(dist));
        graph.add_edge(p2_index, p1_index, Edge::WALK(dist));
    }

    // Add cheat path edges directly to graph
    for (node_a, node_b) in cheat_edges {
        let pos_a = graph[node_a].point;
        let pos_b = graph[node_b].point;
        let dx = pos_b.x - pos_a.x;
        let dy = pos_b.y - pos_a.y;
        let dist = (dx * dx + dy * dy).sqrt();

        graph.add_edge(node_a, node_b, Edge::WALK(dist));
        graph.add_edge(node_b, node_a, Edge::WALK(dist));
    }
    Some(())
}

#[wasm_bindgen(skip_typescript)]
pub fn prepare(g_js: JsValue, exclude_maps: Option<JsValue>) {
    // Convert 'G' to a variable we can use
    let g: GData = from_value(g_js).ok().unwrap();

    // Convert excluded maps to a variable we can use
    let excluded_maps: Vec<String> = match exclude_maps {
        Some(v) => from_value(v).unwrap_or_default(),
        None => Vec::new(),
    };

    for (map_name, map) in &g.maps {
        if map.ignore.is_some() || excluded_maps.contains(map_name) {
            continue; // Skip ignored maps
        }

        // Make the grid
        prepare_map(&g, map_name);
    }

    let mut node_map = NODE_MAP.write().ok().unwrap();
    node_map.clear();
    node_map.shrink_to_fit();

    ensure_map_topology();
}

pub fn get_path_nodes(
    map_from_name: &str,
    // TODO: Add instance
    x_from: f32,
    y_from: f32,
    map_to_name: &str,
    // TODO: Add instance
    x_to: f32,
    y_to: f32,
    speed: Option<f32>,
) -> Option<Vec<PathNode>> {
    let base_speed = speed.unwrap_or(50.0);

    let graph = GRAPH.read().ok().unwrap();

    if map_from_name == map_to_name
        && can_walk_path(
            map_from_name,
            x_from as i32,
            y_from as i32,
            x_to as i32,
            y_to as i32,
        )
    {
        // The path is walkable
        let path = vec![PathNode {
            map: map_to_name.to_string(),
            x: x_to,
            y: y_to,
            method: "move",
            spawn: None,
            key: None,
        }];
        return Some(path);
    }

    // Find closest existing nodes
    let start_node = match find_closest_node(&graph, map_from_name, x_from, y_from) {
        Some(node) => node,
        None => return None,
    };
    let end_node = match find_closest_node(&graph, map_to_name, x_to, y_to) {
        Some(node) => node,
        None => return None,
    };

    ensure_map_topology();

    let start_map_id = match MAP_INDICES.read().ok().unwrap().get(map_from_name) {
        Some(&id) => id,
        None => return None,
    };
    let end_map_id = match MAP_INDICES.read().ok().unwrap().get(map_to_name) {
        Some(&id) => id,
        None => return None,
    };

    let map_count = MAP_NAMES.read().ok().unwrap().len();
    let map_distances = MAP_DISTANCES.read().ok().unwrap();

    let direct_map_dist = if start_map_id < map_count && end_map_id < map_count {
        map_distances[start_map_id * map_count + end_map_id]
    } else {
        f32::INFINITY
    };

    if start_map_id != end_map_id && direct_map_dist.is_infinite() {
        return None;
    }

    let map_exits = MAP_EXITS.read().ok().unwrap();
    let map_names = MAP_NAMES.read().ok().unwrap();

    // A* pathfinding
    let result = astar(
        &*graph,
        start_node,
        |node| node == end_node, // goal condition
        |edge| match edge.weight() {
            Edge::WALK(dist) => *dist / base_speed,
            Edge::TRANSPORT(_) => 3.2, // 3.2s penalty_cd
            Edge::TOWN => 3.812,       // 3s for channel + 812ms penalty_cd
            Edge::DOOR(_) => {
                let mut cost = 0.812; // 812ms penalty_cd

                // Don't penalize if we're going to or starting from the bank
                if map_from_name.starts_with("bank") || map_to_name.starts_with("bank") {
                    return cost;
                }

                // Penalize walking through the bank
                let source = &graph[edge.source()];
                let target = &graph[edge.target()];
                let source_is_bank = map_names
                    .get(source.map_id)
                    .map_or(false, |n| n.starts_with("bank"));
                let target_is_bank = map_names
                    .get(target.map_id)
                    .map_or(false, |n| n.starts_with("bank"));
                if !source_is_bank && target_is_bank {
                    cost += BANK_PENALTY;
                }
                cost
            }
            Edge::ENTER(_) => 0.812, // 812ms penalty_cd
            Edge::LEAVE(_) => 0.812, // 812ms penalty_cd
        },
        |node| {
            let current = &graph[node];
            let goal = &graph[end_node];

            if current.map_id == goal.map_id {
                let dx = goal.point.x - current.point.x;
                let dy = goal.point.y - current.point.y;
                return (dx * dx + dy * dy).sqrt() / base_speed;
            }

            let mut min_h = f32::MAX;
            if let Some(exits) = map_exits.get(current.map_id) {
                for exit in exits {
                    if exit.target_map_id < map_count && goal.map_id < map_count {
                        let rem = map_distances[exit.target_map_id * map_count + goal.map_id];
                        if !rem.is_infinite() {
                            let dx = exit.point.x - current.point.x;
                            let dy = exit.point.y - current.point.y;
                            let h = (dx * dx + dy * dy).sqrt() / base_speed + exit.cost + rem;
                            if h < min_h {
                                min_h = h;
                            }
                        }
                    }
                }
            }

            if min_h < f32::MAX {
                min_h
            } else if current.map_id < map_count && goal.map_id < map_count {
                let d = map_distances[current.map_id * map_count + goal.map_id];
                if d.is_infinite() {
                    0.0
                } else {
                    d
                }
            } else {
                0.0
            }
        },
    );

    match result {
        Some((_cost, path)) => Some(simplify_path(&build_path(
            &graph,
            &path,
            map_to_name,
            x_to,
            y_to,
        ))),
        None => None, // No path found
    }
}

#[wasm_bindgen(js_name = getPath, skip_typescript)]
pub fn get_path(
    map_from_name: &str,
    // TODO: Add instance
    x_from: f32,
    y_from: f32,
    map_to_name: &str,
    // TODO: Add instance
    x_to: f32,
    y_to: f32,
    speed: Option<f32>,
) -> JsValue {
    match get_path_nodes(
        map_from_name,
        x_from,
        y_from,
        map_to_name,
        x_to,
        y_to,
        speed,
    ) {
        Some(path) => serde_wasm_bindgen::to_value(&path).ok().unwrap(),
        None => JsValue::NULL,
    }
}

fn simplify_path(path: &[PathNode]) -> Vec<PathNode> {
    if path.is_empty() {
        return vec![];
    }

    let mut simplified = vec![path[0].clone()];

    let mut i = 0;
    while i < path.len() - 1 {
        // Find the furthest node we can walk to directly from simplified's last node
        let from_node = simplified.last().unwrap();

        let mut furthest = i + 1;
        for j in (i + 2..path.len()).rev() {
            let to_node = &path[j];

            if from_node.map == to_node.map
                && can_walk_path(
                    &from_node.map,
                    from_node.x as i32,
                    from_node.y as i32,
                    to_node.x as i32,
                    to_node.y as i32,
                )
            {
                furthest = j;
                break;
            }
        }

        let mut next_node = path[furthest].clone();

        // If we skipped nodes, we are just moving to this node directly
        if furthest > i + 1 {
            next_node.method = "move";
            next_node.spawn = None;
            next_node.key = None;
        }
        simplified.push(next_node);
        i = furthest;
    }

    simplified
}

fn build_path(
    graph: &Graph<Node, Edge>,
    path: &[NodeIndex],
    map_to_name: &str,
    x_to: f32,
    y_to: f32,
) -> Vec<PathNode> {
    let mut path_nodes: Vec<PathNode> = path
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let node = &graph[idx];

            // Get the method from the edge leading TO this node
            let edge = if i > 0 {
                graph
                    .find_edge(path[i - 1], idx)
                    .and_then(|edge_idx| graph.edge_weight(edge_idx))
            } else {
                None
            };

            PathNode {
                map: get_map_name(node.map_id).unwrap(),
                x: node.point.x,
                y: node.point.y,
                method: match edge {
                    Some(Edge::TOWN) => "town",
                    Some(Edge::DOOR(_)) => "door",
                    Some(Edge::TRANSPORT(_)) => "transport",
                    Some(Edge::ENTER(_)) => "enter",
                    Some(Edge::LEAVE(_)) => "leave",
                    _ => "move",
                },
                spawn: match edge {
                    Some(Edge::DOOR(spawn)
                    | Edge::TRANSPORT(spawn)
                    | Edge::LEAVE(spawn)) => Some(*spawn),
                    Some(Edge::ENTER(data)) => Some(data.0),
                    _ => None,
                },
                key: match edge {
                    Some(Edge::ENTER(data)) => data.1.clone(),
                    _ => None,
                },
            }
        })
        .collect();

    // Add the final node
    if path_nodes
        .last()
        .map_or(true, |n| n.x != x_to || n.y != y_to || n.map != map_to_name)
    {
        path_nodes.push(PathNode {
            map: map_to_name.to_string(),
            x: x_to,
            y: y_to,
            method: "move",
            spawn: None,
            key: None,
        });
    }

    path_nodes
}

#[inline]
fn is_walkable_grid(grid: &CDT, x: f32, y: f32) -> bool {
    let point = Point2::new(x, y);

    match grid.locate(point) {
        spade::PositionInTriangulation::OnFace(face) => {
            grid.face(face).adjacent_edges().iter().any(|e| {
                grid.undirected_edge(e.fix().as_undirected())
                    .data()
                    .data()
                    .reachable
            })
        }
        spade::PositionInTriangulation::OnEdge(edge) => {
            grid.undirected_edge(edge.as_undirected())
                .data()
                .data()
                .reachable
        }
        spade::PositionInTriangulation::OnVertex(_) => true,
        spade::PositionInTriangulation::OutsideOfConvexHull(_) => false,
        spade::PositionInTriangulation::NoTriangulation => false,
    }
}

#[wasm_bindgen(js_name = isWalkable, skip_typescript)]
pub fn is_walkable(map_name: &str, x: f32, y: f32) -> bool {
    let grids = GRIDS.read().ok().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return false,
    };

    is_walkable_grid(grid, x, y)
}

#[wasm_bindgen(js_name = canWalkPath, skip_typescript)]
pub fn can_walk_path(map_name: &str, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
    let cheat_paths = CHEAT_PATHS.read().ok().unwrap();
    for cp in cheat_paths.iter() {
        if cp.map == map_name
            && is_walkable(map_name, cp.x1, cp.y1)
            && is_walkable(map_name, cp.x2, cp.y2)
        {
            let cx1 = cp.x1.trunc() as i32;
            let cy1 = cp.y1.trunc() as i32;
            let cx2 = cp.x2.trunc() as i32;
            let cy2 = cp.y2.trunc() as i32;

            if (x1 == cx1 && y1 == cy1 && x2 == cx2 && y2 == cy2)
                || (x1 == cx2 && y1 == cy2 && x2 == cx1 && y2 == cy1)
            {
                return true;
            }
        }
    }

    let grids = GRIDS.read().ok().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return false,
    };

    if x1 == x2 && y1 == y2 {
        return is_walkable_grid(grid, x1 as f32, y1 as f32);
    }

    let p1 = Point2::new(x1 as f32, y1 as f32);
    let p2 = Point2::new(x2 as f32, y2 as f32);

    if grid.intersects_constraint(p1, p2) {
        return false;
    }

    // TODO: intersects_constraint doesn't always catch everything
    let dx = (x2 - x1) as f32;
    let dy = (y2 - y1) as f32;
    let dist = (dx * dx + dy * dy).sqrt();

    let step_size = 4.0;
    if dist > 0.0 {
        let steps = (dist / step_size).ceil() as usize;
        for i in 0..=steps {
            let t = (i as f32) / (steps as f32);
            let x = x1 as f32 + t * dx;
            let y = y1 as f32 + t * dy;
            if !is_walkable_grid(grid, x, y) {
                return false;
            }
        }
    }

    true
}

pub fn get_safe_walk_node(
    map_name: &str,
    x_from: f32,
    y_from: f32,
    x_to: f32,
    y_to: f32,
) -> Option<SafeWalkPoint> {
    if !is_walkable(map_name, x_from, y_from) {
        // Starting position is not walkable
        return None;
    }

    if x_from == x_to && y_from == y_to {
        // Same position
        return Some(SafeWalkPoint {
            map: map_name.to_string(),
            x: x_to,
            y: y_to,
        });
    }

    // Check cheat paths, if it overlaps, consider it safe to walk (even though it's not)
    let cheat_paths = CHEAT_PATHS.read().ok().unwrap();
    for cp in cheat_paths.iter() {
        if cp.map == map_name
            && is_walkable(map_name, cp.x1, cp.y1)
            && is_walkable(map_name, cp.x2, cp.y2)
        {
            let cx1 = cp.x1.trunc() as i32;
            let cy1 = cp.y1.trunc() as i32;
            let cx2 = cp.x2.trunc() as i32;
            let cy2 = cp.y2.trunc() as i32;
            let x1 = x_from.trunc() as i32;
            let y1 = y_from.trunc() as i32;
            let x2 = x_to.trunc() as i32;
            let y2 = y_to.trunc() as i32;

            if (x1 == cx1 && y1 == cy1 && x2 == cx2 && y2 == cy2)
                || (x1 == cx2 && y1 == cy2 && x2 == cx1 && y2 == cy1)
            {
                return Some(SafeWalkPoint {
                    map: map_name.to_string(),
                    x: x_to,
                    y: y_to,
                });
            }
        }
    }

    let grids = GRIDS.read().ok().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return None,
    };

    let p1 = Point2::new(x_from, y_from);
    let p2 = Point2::new(x_to, y_to);

    let dx = x_to - x_from;
    let dy = y_to - y_from;
    let dist = (dx * dx + dy * dy).sqrt();

    if dist <= f32::EPSILON {
        // Essentially the same position
        return Some(SafeWalkPoint {
            map: map_name.to_string(),
            x: x_from,
            y: y_from,
        });
    }

    let direct_blocked = grid.intersects_constraint(p1, p2);
    if !direct_blocked {
        // Double-check it's walkable
        let step_size = 2.0;
        let steps = (dist / step_size).ceil() as usize;
        let mut fully_walkable = true;
        for i in 1..=steps {
            let t = (i as f32) / (steps as f32);
            let x = x_from + t * dx;
            let y = y_from + t * dy;
            if !is_walkable_grid(grid, x, y) {
                fully_walkable = false;
                break;
            }
        }
        if fully_walkable {
            return Some(SafeWalkPoint {
                map: map_name.to_string(),
                x: x_to,
                y: y_to,
            });
        }
    }

    let step_size = 2.0;
    let steps = (dist / step_size).ceil() as usize;

    let mut safe_ratio = 0.0;
    let mut last_safe_point = p1;
    let mut blocked_ratio = 1.0;

    for i in 1..=steps {
        let current_ratio = ((i as f32) * step_size / dist).min(1.0);
        let current_point = Point2::new(x_from + current_ratio * dx, y_from + current_ratio * dy);

        let blocked = grid.intersects_constraint(last_safe_point, current_point)
            || !is_walkable_grid(grid, current_point.x, current_point.y);

        if blocked {
            blocked_ratio = current_ratio;
            break;
        } else {
            safe_ratio = current_ratio;
            last_safe_point = current_point;
        }
    }

    for _ in 0..12 {
        let mid_ratio = (safe_ratio + blocked_ratio) * 0.5;
        let mid_point = Point2::new(x_from + mid_ratio * dx, y_from + mid_ratio * dy);

        if is_walkable_grid(grid, mid_point.x, mid_point.y)
            && !grid.intersects_constraint(last_safe_point, mid_point)
        {
            safe_ratio = mid_ratio;
            last_safe_point = mid_point;
        } else {
            blocked_ratio = mid_ratio;
        }
    }

    Some(SafeWalkPoint {
        map: map_name.to_string(),
        x: last_safe_point.x,
        y: last_safe_point.y,
    })
}

#[wasm_bindgen(js_name = getSafeWalkTo, skip_typescript)]
pub fn get_safe_walk_to(
    map_name: &str,
    x_from: f32,
    y_from: f32,
    x_to: f32,
    y_to: f32,
) -> JsValue {
    match get_safe_walk_node(map_name, x_from, y_from, x_to, y_to) {
        Some(point) => serde_wasm_bindgen::to_value(&point).ok().unwrap(),
        None => JsValue::NULL,
    }
}

