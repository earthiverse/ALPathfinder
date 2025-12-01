use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

#[derive(Debug)]
pub struct GDoor {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub map_to: String,
    pub spawn_to: u8,
    pub spawn_from: u8,
}

impl<'de> Deserialize<'de> for GDoor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: serde_json::Value = Deserialize::deserialize(deserializer)?;

        if let Some(arr) = value.as_array() {
            if arr.len() >= 7 {
                let x = arr[0]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid x"))?
                    as f32;
                let y = arr[1]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid y"))?
                    as f32;
                let width = arr[2]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid width"))?
                    as f32;
                let height = arr[3]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid height"))?
                    as f32;
                let map_to = arr[4]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("Invalid map_to"))?
                    .to_string();
                let spawn_to = arr[5]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid spawn_to"))?
                    as u8;
                let spawn_from = arr[6]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid spawn_from"))?
                    as u8;

                return Ok(GDoor {
                    x,
                    y,
                    width,
                    height,
                    map_to,
                    spawn_to,
                    spawn_from,
                });
            }
        }

        Err(serde::de::Error::custom(
            "Door array must have at least 7 elements",
        ))
    }
}

fn deserialize_doors<'de, D>(deserializer: D) -> Result<Vec<GDoor>, D::Error>
where
    D: Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = Deserialize::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

#[derive(Debug)]
pub struct GSpawn {
    pub x: f32,
    pub y: f32,
}

impl<'de> Deserialize<'de> for GSpawn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: serde_json::Value = Deserialize::deserialize(deserializer)?;

        if let Some(arr) = value.as_array() {
            if arr.len() >= 2 {
                let x = arr[0]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid x"))?
                    as f32;
                let y = arr[1]
                    .as_f64()
                    .ok_or_else(|| serde::de::Error::custom("Invalid y"))?
                    as f32;

                return Ok(GSpawn { x, y });
            }
        }

        Err(serde::de::Error::custom(
            "Spawn array must have at least 2 elements",
        ))
    }
}

fn deserialize_spawns<'de, D>(deserializer: D) -> Result<Vec<GSpawn>, D::Error>
where
    D: Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = Deserialize::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

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
    #[serde(deserialize_with = "deserialize_doors")]
    pub doors: Vec<GDoor>,
    pub ignore: Option<bool>,
    pub name: String,
    pub npcs: Option<Vec<GNpc>>,
    #[serde(deserialize_with = "deserialize_spawns")]
    pub spawns: Vec<GSpawn>,
}

#[derive(Deserialize, Debug)]
pub struct GNpc {
    pub id: String,
    pub position: Option<Vec<f32>>,
    pub positions: Option<Vec<Vec<f32>>>,
}

#[derive(Deserialize, Debug)]
pub struct GData {
    pub geometry: HashMap<String, GGeometry>,
    pub maps: HashMap<String, GMap>,
    pub version: u32,
}
