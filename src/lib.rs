use clipper2_rust::{ClipType, Clipper64, FillRule, PathType, Paths64, Point};
use once_cell::sync::Lazy;
use petgraph::algo::astar;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::Serialize;
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

mod g;
use crate::g::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
import type { GData, MapKey } from "typed-adventureland";

export interface PathNode {
  map: MapKey;
  x: number;
  y: number;
  method: "move" | "town" | "door" | "transport" | "enter" | "leave" | "unknown";
  spawn?: number;
}

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

export function isWalkable(map: MapKey, x: number, y: number): boolean;

export function prepare(g: GData, exclude_maps?: MapKey[]): void;"#;

#[derive(Serialize)]
struct PathNode {
    map: String,
    x: f32,
    y: f32,
    method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    spawn: Option<u8>,
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

#[derive(Copy, Clone)]
enum Edge {
    WALK,
    TOWN,
    DOOR(u8),
    TRANSPORT(u8),
    ENTER(u8),
    LEAVE(u8),
}

static MAP_NAMES: RwLock<Vec<String>> = RwLock::new(Vec::new());

static GRAPH: Lazy<RwLock<Graph<Node, Edge>>> = Lazy::new(|| RwLock::new(Graph::new()));

static MAP_INDICES: RwLock<FxHashMap<String, usize>> =
    RwLock::new(HashMap::with_hasher(FxBuildHasher));

static NODE_MAP: RwLock<FxHashMap<Node, NodeIndex>> =
    RwLock::new(HashMap::with_hasher(FxBuildHasher));

static GRIDS: RwLock<FxHashMap<String, CDT>> = RwLock::new(HashMap::with_hasher(FxBuildHasher));

#[wasm_bindgen(skip_typescript)]
pub fn clear() {
    MAP_NAMES.write().ok().unwrap().clear();
    GRAPH.write().ok().unwrap().clear();
    MAP_INDICES.write().ok().unwrap().clear();
    NODE_MAP.write().ok().unwrap().clear();
    GRIDS.write().ok().unwrap().clear();
}

fn get_or_add_node(node: Node) -> NodeIndex {
    let mut node_map = NODE_MAP.write().ok().unwrap();
    *node_map
        .entry(node.clone())
        .or_insert_with(|| GRAPH.write().ok().unwrap().add_node(node))
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

fn find_closest_node(
    graph: &Graph<Node, Edge>,
    map_name: &str,
    x: f32,
    y: f32,
) -> Option<NodeIndex> {
    let target_map_id = *MAP_INDICES.read().ok().unwrap().get(map_name)?;
    let mut closest_index: Option<NodeIndex> = None;
    let mut min_distance = f32::MAX;

    for node_index in graph.node_indices() {
        let node = &graph[node_index];
        if node.map_id != target_map_id {
            continue;
        }

        let dx = node.point.x - x;
        let dy = node.point.y - y;
        let distance = dx * dx + dy * dy;

        if distance >= min_distance {
            continue;
        }
        if !can_walk_path(
            map_name,
            node.point.x.trunc() as i32,
            node.point.y.trunc() as i32,
            x.trunc() as i32,
            y.trunc() as i32,
        ) {
            continue;
        }
        min_distance = distance;
        closest_index = Some(node_index);
        if distance <= 1.0 {
            return closest_index;
        }
    }

    return closest_index;
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

        // Add nodes and edges for doors
        let nearby = triangulation.get_vertices_in_rectangle(
            Point2::new(
                spawn_x - half_w - DOOR_RADIUS,
                door.y - door.height - DOOR_RADIUS,
            ),
            Point2::new(spawn_x + half_w + DOOR_RADIUS, door.y + DOOR_RADIUS),
        );
        let edge = if door.requires_key {
            Edge::ENTER(door.spawn_to)
        } else {
            Edge::DOOR(door.spawn_to)
        };
        for n in nearby {
            let n_index = n.data().node_index;

            GRAPH
                .write()
                .ok()
                .unwrap()
                .add_edge(n_index, destination_node_index, edge);
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
                .add_edge(index, destination_node_index, edge);
        }
    }

    // Add nodes for transporters
    for npc in map.npcs.as_ref().unwrap() {
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

    // Add leave logic (leaving sends you to spawn 0 on main)
    if map_name == "jail" || map_name == "cyberland" {
        let destination_map_id = get_or_create_map_id("main");
        let &[dsx, dsy] = g.maps.get("main")?.spawns.get(0)?;
        let destination_node_index = get_or_add_node(Node {
            map_id: destination_map_id,
            point: Point2::new(dsx, dsy),
        });
        let mut graph = GRAPH.write().ok().unwrap();
        for vertice in triangulation.vertices() {
            let node_index = vertice.data().node_index;
            graph.add_edge(node_index, destination_node_index, Edge::LEAVE(0));
        }
    }

    // Add edges to town spawn
    let town_spawn_index = triangulation.vertex(*spawns.get(0)?).data().node_index;

    let mut graph = GRAPH.write().ok().unwrap();
    for vertice in triangulation.vertices() {
        let node_index = vertice.data().node_index;
        if node_index == town_spawn_index {
            continue;
        }

        graph.add_edge(node_index, town_spawn_index, Edge::TOWN);
    }

    flood_fill_holes(&mut triangulation, spawns);

    // Add the grid to the global list

    // Add all nodes to graph
    for edge in triangulation.undirected_edges() {
        if !edge.data().data().reachable {
            continue;
        }

        let [p1, p2] = edge.vertices();

        let p1_index = p1.data().node_index;

        let p2_index = p2.data().node_index;

        graph.add_edge(p1_index, p2_index, Edge::WALK);
        graph.add_edge(p2_index, p1_index, Edge::WALK);
    }
    GRIDS
        .write()
        .ok()
        .unwrap()
        .insert(map_name.to_string(), triangulation);
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
        }];
        return serde_wasm_bindgen::to_value(&path).ok().unwrap();
    }

    // Find closest existing nodes
    let start_node = match find_closest_node(&graph, map_from_name, x_from, y_from) {
        Some(node) => node,
        None => return JsValue::NULL,
    };
    let end_node = match find_closest_node(&graph, map_to_name, x_to, y_to) {
        Some(node) => node,
        None => return JsValue::NULL,
    };

    // A* pathfinding
    let result = astar(
        &*graph,
        start_node,
        |node| node == end_node, // goal condition
        |edge| match edge.weight() {
            Edge::WALK => {
                let source = &graph[edge.source()];
                let target = &graph[edge.target()];

                let dx = target.point.x - source.point.x;
                let dy = target.point.y - source.point.y;
                (dx * dx + dy * dy).sqrt() / base_speed
            }
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
                let source_name = get_map_name(source.map_id).unwrap_or_default();
                let target_name = get_map_name(target.map_id).unwrap_or_default();
                if !source_name.starts_with("bank") && target_name.starts_with("bank") {
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

            if current.map_id != goal.map_id {
                return 100_000.0; // We need to get to another map
            }

            let dx = goal.point.x - current.point.x;
            let dy = goal.point.y - current.point.y;
            return (dx * dx + dy * dy).sqrt() / base_speed;
        },
    );

    match result {
        Some((_cost, path)) => {
            // Convert path to something you can return to JS
            // path is Vec<NodeIndex>
            serialize_path(
                &graph,
                simplify_path(&graph, &path),
                map_to_name,
                x_to,
                y_to,
            )
        }
        None => JsValue::NULL, // No path found
    }
}

fn simplify_path(graph: &Graph<Node, Edge>, path: &[NodeIndex]) -> Vec<NodeIndex> {
    if path.is_empty() {
        return vec![];
    }

    let mut simplified = vec![path[0]];

    let mut i = 0;
    while i < path.len() - 1 {
        // Find the furthest node we can walk to directly from simplified's last node
        let from = *simplified.last().unwrap();
        let from_node = &graph[from];
        let from_map = get_map_name(from_node.map_id).unwrap_or_default();

        let mut furthest = i + 1;
        for j in (i + 2..path.len()).rev() {
            let to_node = &graph[path[j]];
            let to_map = get_map_name(to_node.map_id).unwrap_or_default();

            if from_map == to_map
                && can_walk_path(
                    &from_map,
                    from_node.point.x as i32,
                    from_node.point.y as i32,
                    to_node.point.x as i32,
                    to_node.point.y as i32,
                )
            {
                furthest = j;
                break;
            }
        }

        simplified.push(path[furthest]);
        i = furthest;
    }

    simplified
}

fn serialize_path(
    graph: &Graph<Node, Edge>,
    path: Vec<NodeIndex>,
    map_to_name: &str,
    x_to: f32,
    y_to: f32,
) -> JsValue {
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
                    .unwrap_or(&Edge::WALK) // Fallback to WALK if edge not found
            } else {
                &Edge::WALK
            };

            PathNode {
                map: get_map_name(node.map_id).unwrap(),
                x: node.point.x,
                y: node.point.y,
                method: match edge {
                    Edge::WALK => "move",
                    Edge::TOWN => "town",
                    Edge::DOOR(_) => "door",
                    Edge::TRANSPORT(_) => "transport",
                    Edge::ENTER(_) => "enter",
                    Edge::LEAVE(_) => "leave",
                },
                spawn: match edge {
                    Edge::DOOR(spawn)
                    | Edge::TRANSPORT(spawn)
                    | Edge::ENTER(spawn)
                    | Edge::LEAVE(spawn) => Some(*spawn),
                    _ => None,
                },
            }
        })
        .collect();

    // Add the final node
    path_nodes.push(PathNode {
        map: map_to_name.to_string(),
        x: x_to,
        y: y_to,
        method: "move",
        spawn: None,
    });

    serde_wasm_bindgen::to_value(&path_nodes).ok().unwrap()
}

#[wasm_bindgen(js_name = isWalkable, skip_typescript)]
pub fn is_walkable(map_name: &str, x: f32, y: f32) -> bool {
    let grids = GRIDS.read().ok().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return false,
    };

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

#[wasm_bindgen(js_name = canWalkPath, skip_typescript)]
pub fn can_walk_path(map_name: &str, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
    let grids = GRIDS.read().ok().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return false,
    };

    if x1 == x2 && y1 == y2 {
        return is_walkable(map_name, x1 as f32, y1 as f32);
    }

    let p1 = Point2::new(x1 as f32, y1 as f32);
    let p2 = Point2::new(x2 as f32, y2 as f32);

    if grid.intersects_constraint(p1, p2) {
        return false;
    }

    // TODO: This is probably not optimal
    // There is an issue if the path is directly aligned along a wall
    if x1 == x2 {
        for y in y1.min(y2)..=y1.max(y2) {
            if !is_walkable(map_name, x1 as f32, y as f32) {
                return false;
            }
        }
    } else if y1 == y2 {
        for x in x1.min(x2)..=x1.max(x2) {
            if !is_walkable(map_name, x as f32, y1 as f32) {
                return false;
            }
        }
    }

    true
}
