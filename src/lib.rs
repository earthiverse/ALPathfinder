use core::cmp::{max, min};
use wasm_bindgen::prelude::*;

mod g;
use crate::g::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

const BASE_H: i32 = 8;
const BASE_V: i32 = 7;
const BASE_VN: i32 = 2;
const UNKNOWN: u8 = 0;
const NOT_WALKABLE: u8 = 1;

pub fn prepare_map(g: &GData, map_name: &String) {
    log(&format!("we parsing {}", map_name));
    // Get the data
    // let map = g.maps.get(map_name).unwrap();
    let geometry = g.geometry.get(map_name).unwrap();

    // Compute important values
    let width = geometry.max_x - geometry.min_x;
    let height = geometry.max_y - geometry.min_y;
    let size: usize = (width * height).try_into().unwrap();

    // Create the grid
    let mut grid: Vec<u8> = vec![UNKNOWN; size];

    // Make the y-lines non-walkable
    log("we making y-lines non-walkable");
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
                        let cell: usize = (y * width + x).try_into().unwrap();
                        grid[cell] = NOT_WALKABLE;
                    }
                }
            }
        }
    }

    // Make the x-lines non-walkable
    log("we making x-lines non-walkable");
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
                        let cell: usize = (y * width + x).try_into().unwrap();
                        grid[cell] = NOT_WALKABLE;
                    }
                }
            }
        }
    }

    // DEBUG Output
    log(&format!("{} is {}x{}!", map_name, width, height));
}

#[wasm_bindgen]
pub fn prepare(g_js: &JsValue) {
    // Convert 'G' to a variable we can use
    let g: GData = g_js.into_serde().unwrap();

    for (map_name, map) in &g.maps {
        // Skip ignored maps
        match map.ignore {
            None => {}
            Some(_v) => continue,
        }

        // Make the grid
        prepare_map(&g, map_name);
    }
}
