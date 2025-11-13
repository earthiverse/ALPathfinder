use bit_vec::BitVec;
use core::cmp::{max, min};
use once_cell::sync::Lazy;
use serde_wasm_bindgen::from_value;
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

static GRIDS: Lazy<RwLock<HashMap<String, Grid>>> = Lazy::new(|| {
    let m = HashMap::new();
    RwLock::new(m)
});

const BASE_H: i32 = 8;
const BASE_V: i32 = 7;
const BASE_VN: i32 = 2;
const UNKNOWN: u8 = 0;
const NOT_WALKABLE: u8 = 1;
const WALKABLE: u8 = 2;

pub fn prepare_map(g: &GData, map_name: &String) {
    // Get the data
    let map = g.maps.get(map_name).unwrap();
    let geometry = g.geometry.get(map_name).unwrap();

    // Compute important values
    let width = geometry.max_x - geometry.min_x;
    let height = geometry.max_y - geometry.min_y;
    let size: usize = (width * height).try_into().unwrap();

    let mut temp_data = vec![UNKNOWN; size];

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
                        temp_data[(y * width + x) as usize] = NOT_WALKABLE;
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
                        temp_data[(y * width + x) as usize] = NOT_WALKABLE;
                    }
                }
            }
        }
    }

    // Fill in the walkable areas
    for spawn in &map.spawns {
        let x = spawn[0].trunc() as i32 - geometry.min_x;
        let y = spawn[1].trunc() as i32 - geometry.min_y;

        if temp_data[(y * width + x) as usize] == WALKABLE {
            // We've already determined this area is walkable
            continue;
        };

        let mut stack: Vec<(i32, i32)> = Vec::new();
        stack.push((y, x));
        while stack.len() > 0 {
            // log("working");
            let (y, mut x) = stack.pop().unwrap();
            while x >= 0 && temp_data[(y * width + x) as usize] == UNKNOWN {
                x -= 1;
            }
            x += 1;
            let mut span_above = false;
            let mut span_below = false;
            while x < width && temp_data[(y * width + x) as usize] == UNKNOWN {
                temp_data[(y * width + x) as usize] = WALKABLE;
                if !span_above && y > 0 && temp_data[((y - 1) * width + x) as usize] == UNKNOWN {
                    stack.push((y - 1, x));
                    span_above = true;
                } else if span_above
                    && y > 0
                    && temp_data[((y - 1) * width + x) as usize] != UNKNOWN
                {
                    span_above = false;
                }

                if !span_below
                    && y < height - 1
                    && temp_data[((y + 1) * width + x) as usize] == UNKNOWN
                {
                    stack.push((y + 1, x));
                    span_below = true;
                } else if span_below
                    && y < height - 1
                    && temp_data[((y + 1) * width + x) as usize] != UNKNOWN
                {
                    span_below = false;
                }
                x += 1;
            }
        }
    }

    // Add the grid to the global list
    let mut grids = GRIDS.write().unwrap();
    grids.insert(
        map_name.to_string(),
        Grid {
            width: width,
            min_x: geometry.min_x,
            min_y: geometry.min_y,
            data: temp_data.iter().map(|&state| state == WALKABLE).collect(),
        },
    );
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

    // TODO: For debugging, remove later
    log(&format!(
        "Prepared {} maps in {}ms!",
        GRIDS.read().unwrap().len(),
        start.elapsed().as_millis()
    ))
}

#[wasm_bindgen]
pub fn is_walkable(map_name: &str, x: i32, y: i32) -> bool {
    let grids = GRIDS.read().unwrap();
    let grid = match grids.get(map_name) {
        Some(g) => g,
        None => return false,
    };

    // Convert the game coordinates to grid coordinates
    let x = x - grid.min_x;
    let y = y - grid.min_y;

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
