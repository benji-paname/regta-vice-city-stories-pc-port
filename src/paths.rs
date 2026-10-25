//! Serialized PS2 VCS path graph. Offsets cross-checked against the local image:
//! 8,387 ten-byte nodes, 3,079 car nodes, 5,308 pedestrian nodes, 17,652 links.
//! Only coordinates and link indices are interpreted; unknown flags stay raw.
use crate::iso::{bytes, u16le, u32le, Result};

pub struct PathNode {
    pub position: [f32; 3],
    pub links: Vec<PathLink>,
    #[allow(dead_code)]
    pub flags: u16,
}

pub struct PathLink {
    pub target: usize,
    /// Native lane counts on either side of this road link. Direction relative
    /// to its reference node is intentionally not assigned a left/right name.
    pub lanes: Option<[u8; 2]>,
    #[allow(dead_code)]
    pub crosses_road: bool,
}

pub struct PathNetwork {
    pub nodes: Vec<PathNode>,
    pub car_nodes: usize,
}

fn pointer(data: &[u8], field: usize, size: usize) -> Result<usize> {
    let p = u32le(data, field)? as usize;
    if p < 32 {
        return Err("Invalid path pointer".into());
    }
    bytes(data, p, size)?;
    Ok(p)
}

impl PathNetwork {
    pub fn nearest_roads(&self, position: [f32; 3], radius: f32) -> Vec<usize> {
        let distance = |i: usize| {
            (self.nodes[i].position[0] - position[0]).powi(2)
                + (self.nodes[i].position[1] - position[1]).powi(2)
        };
        let mut ids: Vec<_> = (0..self.car_nodes)
            .filter(|&i| {
                distance(i) <= radius * radius
                    && (self.nodes[i].position[2] - position[2]).abs() < 3.0
                    && self.nodes[i]
                        .links
                        .iter()
                        .any(|link| link.target < self.car_nodes && link.two_way())
            })
            .collect();
        ids.sort_by(|&a, &b| distance(a).total_cmp(&distance(b)));
        ids
    }

    pub fn load(game: &[u8]) -> Result<Self> {
        let paths = pointer(game, 0x20, 0x7ba8)?;
        let count = u32le(game, paths + 12)? as usize;
        let car_nodes = u32le(game, paths + 16)? as usize;
        let ped_nodes = u32le(game, paths + 20)? as usize;
        let link_count = u16le(game, paths + 26)? as usize;
        if count == 0 || count > 16384 || car_nodes + ped_nodes != count {
            return Err("Invalid VCS path counts".into());
        }
        let table = pointer(game, paths, count * 10)?;
        let links = pointer(game, paths + 0x7ba0, link_count * 2)?;
        let road_connections = u16le(game, paths + 28)? as usize;
        let road_count = u32le(game, paths + 32)? as usize;
        if road_connections > link_count || road_count > 32767 {
            return Err("Invalid road link counts".into());
        }
        let road_indices = if road_connections > 0 {
            pointer(game, paths + 8, road_connections * 2)?
        } else {
            0
        };
        let roads = if road_count > 0 {
            pointer(game, paths + 4, road_count * 4)?
        } else {
            0
        };
        let mut nodes = Vec::with_capacity(count);
        let mut decoded_links = 0;
        for id in 0..count {
            let at = table + id * 10;
            let first = u16le(game, at + 6)? as usize;
            let flags = u16le(game, at + 8)?;
            let n = (flags & 15) as usize;
            if first + n > link_count {
                return Err("Path node links exceed table".into());
            }
            let mut connections = Vec::with_capacity(n);
            for index in first..first + n {
                let raw = u16le(game, links + index * 2)?;
                let target = (raw & 0x3fff) as usize;
                if target >= count {
                    return Err("Invalid path link destination".into());
                }
                let lanes = if id < car_nodes {
                    if index >= road_connections || target >= car_nodes {
                        return Err("Road connection exceeds native table".into());
                    }
                    let road = u16le(game, road_indices + index * 2)? as usize;
                    if road >= road_count {
                        return Err("Invalid road link index".into());
                    }
                    let reference = (u16le(game, roads + road * 4)? & 0x3fff) as usize;
                    if reference != id && reference != target {
                        return Err("Road link reference does not match endpoints".into());
                    }
                    // PS2 0x430F68..0x430F88 selects bits 0..2 or 3..5 of
                    // +2 and rejects zero lanes; 0x40A7A0 sums both sides.
                    let flags = u16le(game, roads + road * 4 + 2)?;
                    Some([(flags & 7) as u8, ((flags >> 3) & 7) as u8])
                } else {
                    None
                };
                connections.push(PathLink {
                    target,
                    lanes,
                    crosses_road: raw & 0x8000 != 0,
                });
            }
            decoded_links += n;
            nodes.push(PathNode {
                position: [
                    u16le(game, at)? as i16 as f32 / 8.0,
                    u16le(game, at + 2)? as i16 as f32 / 8.0,
                    game[at + 4] as i8 as f32,
                ],
                links: connections,
                flags: flags & !15,
            });
        }
        if decoded_links != link_count {
            return Err("Path link totals disagree".into());
        }
        Ok(Self { nodes, car_nodes })
    }

    pub fn nearest_pedestrians(&self, position: [f32; 3], radius: f32) -> Vec<usize> {
        let distance = |n: &PathNode| {
            (n.position[0] - position[0]).powi(2) + (n.position[1] - position[1]).powi(2)
        };
        let mut ids: Vec<_> = (self.car_nodes..self.nodes.len())
            .filter(|&i| {
                let node = &self.nodes[i];
                distance(node) <= radius * radius
                    && (node.position[2] - position[2]).abs() < 5.0
                    && node.links.iter().any(|link| link.target >= self.car_nodes)
            })
            .collect();
        ids.sort_by(|&a, &b| distance(&self.nodes[a]).total_cmp(&distance(&self.nodes[b])));
        ids
    }
}

impl PathLink {
    /// First traffic pass uses verified two-way roads. One-way direction and
    /// signal rules will be enabled once their orientation is cross-checked.
    pub fn two_way(&self) -> bool {
        self.lanes.is_some_and(|n| n[0] > 0 && n[1] > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut data = vec![0; 0x8100];
        for (offset, value) in [
            (0x20, 0x100u32),
            (0x100, 0x7d00),
            (0x10c, 3),
            (0x110, 1),
            (0x114, 2),
            (0x7ca0, 0x8000),
        ] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[0x11a..0x11c].copy_from_slice(&2u16.to_le_bytes());
        for (i, (x, y, z, first, count)) in [
            (0i16, 0i16, 0i8, 0u16, 0u16),
            (-80, 160, -3, 0, 0x4001),
            (80, 160, -3, 1, 1),
        ]
        .into_iter()
        .enumerate()
        {
            let at = 0x7d00 + i * 10;
            data[at..at + 2].copy_from_slice(&x.to_le_bytes());
            data[at + 2..at + 4].copy_from_slice(&y.to_le_bytes());
            data[at + 4] = z as u8;
            data[at + 6..at + 8].copy_from_slice(&first.to_le_bytes());
            data[at + 8..at + 10].copy_from_slice(&count.to_le_bytes());
        }
        data[0x8000..0x8004].copy_from_slice(&[2, 0x80, 1, 0]);
        data
    }

    #[test]
    fn native_signed_coordinates_and_connection_flags_decode() {
        let graph = PathNetwork::load(&fixture()).unwrap();
        assert_eq!(graph.car_nodes, 1);
        assert_eq!(graph.nodes[1].position, [-10.0, 20.0, -3.0]);
        assert_eq!(graph.nodes[1].flags, 0x4000);
        assert_eq!(graph.nodes[1].links[0].target, 2);
        assert!(graph.nodes[1].links[0].crosses_road);
        assert_eq!(
            graph.nearest_pedestrians([-9.0, 20.0, -3.0], 30.0),
            vec![1, 2]
        );
        assert!(graph
            .nearest_pedestrians([-9.0, 20.0, 10.0], 30.0)
            .is_empty());
    }

    #[test]
    fn invalid_link_ranges_and_destinations_are_rejected() {
        let mut data = fixture();
        data[0x8000] = 3;
        assert!(PathNetwork::load(&data).is_err());
        let mut data = fixture();
        data[0x7d00 + 16] = 2;
        assert!(PathNetwork::load(&data).is_err());
    }

    #[test]
    fn malformed_path_image_fails_without_indexing_out_of_bounds() {
        assert!(PathNetwork::load(&[0; 64]).is_err());
        assert!(PathNetwork::load(&[]).is_err());
    }
}
