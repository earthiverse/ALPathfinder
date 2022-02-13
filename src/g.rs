use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct GGeometry {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
    pub x_lines: Option<Vec<Vec<i32>>>,
    pub y_lines: Option<Vec<Vec<i32>>>,
}

#[derive(Deserialize, Debug)]
pub struct GMap {
    pub ignore: Option<bool>,
    pub name: String,
    pub pvp: Option<bool>,
    pub spawns: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
pub struct GData {
    pub geometry: HashMap<String, GGeometry>,
    pub maps: HashMap<String, GMap>,
    pub version: u64,
}
