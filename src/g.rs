use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

pub struct GDoor {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub map_to: String,
    pub spawn_to: u8,
    pub spawn_from: u8,
    pub requires_key: bool,
    pub key: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DoorFlag {
    Key(String),
    Other(serde::de::IgnoredAny),
}

struct GDoorVisitor;

impl<'de> serde::de::Visitor<'de> for GDoorVisitor {
    type Value = Option<GDoor>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a door array with at least 7 elements")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let x: f32 = match seq.next_element()? {
            Some(v) => v,
            None => return Ok(None),
        };
        let y: f32 = match seq.next_element()? {
            Some(v) => v,
            None => return Ok(None),
        };
        let width: f32 = match seq.next_element()? {
            Some(v) => v,
            None => return Ok(None),
        };
        let height: f32 = match seq.next_element()? {
            Some(v) => v,
            None => return Ok(None),
        };
        let map_to: String = match seq.next_element()? {
            Some(v) => v,
            None => return Ok(None),
        };
        let spawn_to: u8 = match seq.next_element()? {
            Some(v) => v,
            None => return Ok(None),
        };
        let spawn_from: u8 = match seq.next_element()? {
            Some(v) => v,
            None => return Ok(None),
        };

        let flag: Option<DoorFlag> = seq.next_element()?;
        let requires_key = matches!(&flag, Some(DoorFlag::Key(k)) if k == "key");
        let key = if requires_key {
            seq.next_element()?
        } else {
            None
        };

        // Drain any trailing array elements
        while let Some(serde::de::IgnoredAny) = seq.next_element()? {}

        Ok(Some(GDoor {
            x,
            y,
            width,
            height,
            map_to,
            spawn_to,
            spawn_from,
            requires_key,
            key,
        }))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some((serde::de::IgnoredAny, serde::de::IgnoredAny)) = map.next_entry()? {}
        Ok(None)
    }
}

impl<'de> Deserialize<'de> for GDoor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(GDoorVisitor)?.ok_or_else(|| {
            serde::de::Error::custom("Door array must have at least 7 elements")
        })
    }
}

struct GDoorOpt(Option<GDoor>);

impl<'de> Deserialize<'de> for GDoorOpt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(GDoorVisitor).map(GDoorOpt)
    }
}

fn deserialize_doors<'de, D>(deserializer: D) -> Result<Vec<GDoor>, D::Error>
where
    D: Deserializer<'de>,
{
    struct DoorsVisitor;

    impl<'de> serde::de::Visitor<'de> for DoorsVisitor {
        type Value = Vec<GDoor>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence of doors")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut doors = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(GDoorOpt(door_opt)) = seq.next_element()? {
                if let Some(door) = door_opt {
                    doors.push(door);
                }
            }
            Ok(doors)
        }
    }

    deserializer.deserialize_seq(DoorsVisitor)
}

#[derive(Deserialize, Default)]
pub struct GGeometry {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
    pub x_lines: Option<Vec<[i32; 3]>>,
    pub y_lines: Option<Vec<[i32; 3]>>,
}

#[derive(Deserialize, Default)]
pub struct GMap {
    #[serde(deserialize_with = "deserialize_doors")]
    pub doors: Vec<GDoor>,
    pub ignore: Option<bool>,
    pub name: String,
    pub npcs: Option<Box<[GMapNpc]>>,
    pub spawns: Box<[[f32; 2]]>,
}

#[derive(Deserialize)]
pub struct GMapNpc {
    pub id: String,
    pub position: Option<[f32; 2]>,
    pub positions: Option<Box<[[f32; 2]]>>,
}

#[derive(Deserialize)]
pub struct GNpc {
    pub places: Option<HashMap<String, u8>>,
}

#[derive(Deserialize, Default)]
pub struct GData {
    pub geometry: HashMap<String, GGeometry>,
    pub maps: HashMap<String, GMap>,
    pub npcs: HashMap<String, GNpc>,
    pub version: u32,
}
