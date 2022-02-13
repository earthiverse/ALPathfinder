use core::cmp::{max, min};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

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
    data: Vec<u8>,
}

lazy_static! {
    static ref GRIDS: Mutex<HashMap<String, Grid>> = {
        let m = HashMap::new();
        Mutex::new(m)
    };
}

const BASE_H: i32 = 8;
const BASE_V: i32 = 7;
const BASE_VN: i32 = 2;
const UNKNOWN: u8 = 1;
const NOT_WALKABLE: u8 = 2;
const WALKABLE: u8 = 3;

pub fn prepare_map(g: &GData, map_name: &String) {
    // log(&format!("Preparing {}...", map_name));
    // let start = instant::Instant::now();

    // Get the data
    let map = g.maps.get(map_name).unwrap();
    let geometry = g.geometry.get(map_name).unwrap();

    // Compute important values
    let width = geometry.max_x - geometry.min_x;
    let height = geometry.max_y - geometry.min_y;
    let size: usize = (width * height).try_into().unwrap();

    // Create the grid
    let mut grid = Grid {
        width: width,
        min_x: geometry.min_x,
        min_y: geometry.min_y,
        data: vec![UNKNOWN; size],
    };

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
                        grid.data[(y * width + x) as usize] = NOT_WALKABLE;
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
                        grid.data[(y * width + x) as usize] = NOT_WALKABLE;
                    }
                }
            }
        }
    }

    // Fill in the walkable areas
    for spawn in &map.spawns {
        let x = spawn[0].trunc() as i32 - geometry.min_x;
        let y = spawn[1].trunc() as i32 - geometry.min_y;

        if grid.data[(y * width + x) as usize] == WALKABLE {
            // We've already determined this area is walkable
            continue;
        };

        let mut stack: Vec<(i32, i32)> = Vec::new();
        stack.push((y, x));
        while stack.len() > 0 {
            // log("working");
            let (y, mut x) = stack.pop().unwrap();
            while x >= 0 && grid.data[(y * width + x) as usize] == UNKNOWN {
                x -= 1;
            }
            x += 1;
            let mut span_above = false;
            let mut span_below = false;
            while x < width && grid.data[(y * width + x) as usize] == UNKNOWN {
                grid.data[(y * width + x) as usize] = WALKABLE;
                if !span_above && y > 0 && grid.data[((y - 1) * width + x) as usize] == UNKNOWN {
                    stack.push((y - 1, x));
                    span_above = true;
                } else if span_above
                    && y > 0
                    && grid.data[((y - 1) * width + x) as usize] != UNKNOWN
                {
                    span_above = false;
                }

                if !span_below
                    && y < height - 1
                    && grid.data[((y + 1) * width + x) as usize] == UNKNOWN
                {
                    stack.push((y + 1, x));
                    span_below = true;
                } else if span_below
                    && y < height - 1
                    && grid.data[((y + 1) * width + x) as usize] != UNKNOWN
                {
                    span_below = false;
                }
                x += 1;
            }
        }
    }

    // Add to hashmap
    let mut grids = GRIDS.lock().unwrap();
    grids.insert(map_name.to_string(), grid);

    // DEBUG Output
    // log(&format!(
    //     "  Prepared grid for {} in {}ms!",
    //     map_name,
    //     start.elapsed().as_millis()
    // ));
}

#[wasm_bindgen]
pub fn prepare(g_js: &JsValue) {
    // Convert 'G' to a variable we can use
    let g: GData = g_js.into_serde().unwrap();

    let start = instant::Instant::now();
    for (map_name, map) in &g.maps {
        // Skip ignored maps
        match map.ignore {
            None => {}
            Some(_v) => continue,
        }

        // Make the grid
        prepare_map(&g, map_name);
    }
    log(&format!(
        "Prepared all maps in {}ms!",
        start.elapsed().as_millis()
    ))
}

#[wasm_bindgen]
pub fn is_walkable(map_name: &str, x_i: i32, y_i: i32) -> bool {
    let grids = GRIDS.lock().unwrap();
    let grid = grids.get(map_name).unwrap();

    // Convert the game coordinates to grid coordinates
    let x = x_i - grid.min_x;
    let y = y_i - grid.min_y;

    let cell = grid.data[(y * grid.width + x) as usize];
    return cell == WALKABLE;
}
