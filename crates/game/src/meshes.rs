//! Helpers for building meshes from lists of points.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::prelude::*;

/// Positions, texture coordinates and triangles, built up and then turned into a mesh.
#[derive(Default, Clone)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn vertex(&mut self, p: [f64; 3], uv: [f64; 2]) -> u32 {
        self.positions.push([p[0] as f32, p[1] as f32, p[2] as f32]);
        self.uvs.push([uv[0] as f32, uv[1] as f32]);
        self.positions.len() as u32 - 1
    }

    pub fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Adds another mesh's triangles.
    pub fn append(&mut self, other: &MeshData) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.uvs.extend_from_slice(&other.uvs);
        self.normals.extend_from_slice(&other.normals);
        if !self.colors.is_empty() || !other.colors.is_empty() {
            self.colors.resize(base as usize, [1.0; 4]);
            self.colors.extend_from_slice(&other.colors);
            self.colors.resize(self.positions.len(), [1.0; 4]);
        }
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    /// Smooth normals from the triangles (like three.js computeVertexNormals).
    pub fn compute_normals(&mut self) {
        let mut normals = vec![Vec3::ZERO; self.positions.len()];
        for t in self.indices.chunks(3) {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            let (pa, pb, pc) = (
                Vec3::from(self.positions[a]),
                Vec3::from(self.positions[b]),
                Vec3::from(self.positions[c]),
            );
            let n = (pb - pa).cross(pc - pa);
            normals[a] += n;
            normals[b] += n;
            normals[c] += n;
        }
        self.normals = normals.into_iter().map(|n| n.normalize_or(Vec3::Y).to_array()).collect();
    }

    /// Adds the back of every triangle too, for strips seen from both sides.
    pub fn double_sided(&mut self) {
        if self.normals.len() != self.positions.len() {
            self.compute_normals();
        }
        let count = self.positions.len() as u32;
        let positions = self.positions.clone();
        self.positions.extend(positions);
        self.uvs.extend(self.uvs.clone());
        let flipped: Vec<[f32; 3]> = self.normals.iter().map(|n| [-n[0], -n[1], -n[2]]).collect();
        self.normals.extend(flipped);
        if !self.colors.is_empty() {
            self.colors.extend(self.colors.clone());
        }
        let back: Vec<u32> = self
            .indices
            .chunks(3)
            .flat_map(|t| [t[0] + count, t[2] + count, t[1] + count])
            .collect();
        self.indices.extend(back);
    }

    /// The finished mesh, kept only on the graphics card.
    pub fn build(self) -> Mesh {
        self.build_with(RenderAssetUsages::RENDER_WORLD)
    }

    /// The finished mesh, also kept in the game's memory so it can be changed later.
    pub fn build_editable(self) -> Mesh {
        self.build_with(RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
    }

    fn build_with(mut self, usage: RenderAssetUsages) -> Mesh {
        if self.normals.len() != self.positions.len() {
            self.compute_normals();
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, usage);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        if !self.colors.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        }
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

impl MeshData {
    /// A Bevy mesh's triangles, moved by `transform`, in one color (linear RGBA).
    pub fn from_mesh(mesh: &Mesh, transform: &Transform, color: [f32; 4]) -> MeshData {
        let mut out = MeshData::default();
        out.add_mesh(mesh, transform, color);
        out
    }

    /// Adds a Bevy mesh's triangles, moved by `transform`, in one color (linear RGBA).
    pub fn add_mesh(&mut self, mesh: &Mesh, transform: &Transform, color: [f32; 4]) {
        use bevy::mesh::VertexAttributeValues as V;
        let base = self.positions.len() as u32;
        let matrix = transform.to_matrix();
        let normal_matrix = bevy::math::Mat3::from_mat4(matrix).inverse().transpose();
        let Some(V::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            return;
        };
        let count = positions.len();
        for p in positions {
            self.positions.push(matrix.transform_point3(Vec3::from(*p)).to_array());
        }
        match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(V::Float32x3(normals)) => {
                for n in normals {
                    self.normals.push((normal_matrix * Vec3::from(*n)).normalize_or(Vec3::Y).to_array());
                }
            }
            _ => self.normals.extend(std::iter::repeat_n([0.0, 1.0, 0.0], count)),
        }
        match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(V::Float32x2(uvs)) => self.uvs.extend_from_slice(uvs),
            _ => self.uvs.extend(std::iter::repeat_n([0.0, 0.0], count)),
        }
        // Keep the color list the same length as the positions once any mesh has colors.
        if self.colors.len() < base as usize {
            self.colors.resize(base as usize, [1.0; 4]);
        }
        self.colors.extend(std::iter::repeat_n(color, count));
        match mesh.indices() {
            Some(indices) => self.indices.extend(indices.iter().map(|i| i as u32 + base)),
            None => self.indices.extend((0..count as u32).map(|i| i + base)),
        }
    }

    /// Gives every triangle its own corners, with the triangle's normal: a faceted look.
    pub fn flat_shaded(self) -> MeshData {
        let mut out = MeshData::default();
        let has_colors = self.colors.len() == self.positions.len();
        for t in self.indices.chunks(3) {
            let p: Vec<Vec3> = t.iter().map(|&i| Vec3::from(self.positions[i as usize])).collect();
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y).to_array();
            for &i in t {
                out.positions.push(self.positions[i as usize]);
                out.normals.push(n);
                out.uvs.push(self.uvs.get(i as usize).copied().unwrap_or([0.0, 0.0]));
                if has_colors {
                    out.colors.push(self.colors[i as usize]);
                }
            }
            let k = out.positions.len() as u32;
            out.indices.extend([k - 3, k - 2, k - 1]);
        }
        out
    }
}

/// An sRGB color (as CSS writes it) in the linear values meshes use.
pub fn linear(rgb: u32) -> [f32; 4] {
    let c = Color::srgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8).to_linear();
    [c.red, c.green, c.blue, 1.0]
}

/// HSL (0..1) in the linear values meshes use.
pub fn linear_hsl(h: f64, s: f64, l: f64) -> [f32; 4] {
    let c = crate::canvas::hsl(h as f32, s as f32, l as f32);
    let lin = Color::srgb(c.0, c.1, c.2).to_linear();
    [lin.red, lin.green, lin.blue, 1.0]
}

/// A cylinder whose top and bottom can differ in size (like three.js CylinderGeometry), centered on the origin.
pub fn tapered_cylinder(top: f32, bottom: f32, height: f32, segments: u32) -> Mesh {
    bevy::shape::ConicalFrustum {
        radius_top: top,
        radius_bottom: bottom,
        height,
    }
    .mesh()
    .resolution(segments)
    .build()
}

/// A shape drawn side-on (x across, y up) and pushed out `depth` along z, from z = 0 to z = depth.
pub fn extrude(shape: &[(f32, f32)], depth: f32) -> MeshData {
    // Work counter-clockwise, so the sides face out.
    let area: f32 = (0..shape.len())
        .map(|i| shape[i].0 * shape[(i + 1) % shape.len()].1 - shape[(i + 1) % shape.len()].0 * shape[i].1)
        .sum();
    let owned: Vec<(f32, f32)> = if area < 0.0 {
        shape.iter().rev().copied().collect()
    } else {
        shape.to_vec()
    };
    let shape = &owned[..];
    let mut m = MeshData::default();
    let n = shape.len();
    // The sides: one flat quad per edge.
    for i in 0..n {
        let (a, b) = (shape[i], shape[(i + 1) % n]);
        let edge = Vec2::new(b.0 - a.0, b.1 - a.1);
        // Outward for a counter-clockwise shape.
        let normal = Vec3::new(edge.y, -edge.x, 0.0).normalize_or(Vec3::Y);
        let base = m.positions.len() as u32;
        for (p, z) in [(a, 0.0), (b, 0.0), (b, depth), (a, depth)] {
            m.positions.push([p.0, p.1, z]);
            m.normals.push(normal.to_array());
            m.uvs.push([z, p.1]);
        }
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    // The two ends: the shape cut into triangles.
    let tris = triangulate(shape);
    for (z, normal, flip) in [(0.0, -1.0, true), (depth, 1.0, false)] {
        let base = m.positions.len() as u32;
        for p in shape.iter() {
            m.positions.push([p.0, p.1, z]);
            m.normals.push([0.0, 0.0, normal]);
            m.uvs.push([p.0, p.1]);
        }
        for t in &tris {
            if flip {
                m.indices.extend([base + t[0], base + t[2], base + t[1]]);
            } else {
                m.indices.extend([base + t[0], base + t[1], base + t[2]]);
            }
        }
    }
    m
}

/// Cuts a simple polygon into triangles by ear clipping; they come back counter-clockwise.
pub fn triangulate(shape: &[(f32, f32)]) -> Vec<[u32; 3]> {
    let mut idx: Vec<u32> = (0..shape.len() as u32).collect();
    let area: f32 = (0..shape.len())
        .map(|i| {
            let (a, b) = (shape[i], shape[(i + 1) % shape.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    if area < 0.0 {
        idx.reverse();
    }
    let mut out = Vec::new();
    let p = |i: u32| Vec2::new(shape[i as usize].0, shape[i as usize].1);
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = idx.len();
        let mut clipped = false;
        for k in 0..n {
            let (a, b, c) = (idx[(k + n - 1) % n], idx[k], idx[(k + 1) % n]);
            let (pa, pb, pc) = (p(a), p(b), p(c));
            let turn = (pb - pa).perp_dot(pc - pb);
            if turn.abs() < 1e-7 {
                // A point on a straight edge (or a repeat): drop it, no triangle needed.
                idx.remove(k);
                clipped = true;
                break;
            }
            if turn < 0.0 {
                continue;
            }
            let inside = idx.iter().any(|&o| {
                if o == a || o == b || o == c {
                    return false;
                }
                let q = p(o);
                (pb - pa).perp_dot(q - pa) > 0.0 && (pc - pb).perp_dot(q - pb) > 0.0 && (pa - pc).perp_dot(q - pc) > 0.0
            });
            if inside {
                continue;
            }
            out.push([a, b, c]);
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn appending_colored_and_uncolored_meshes_preserves_vertex_alignment() {
        use super::{MeshData, Transform};
        use bevy::prelude::Cuboid;

        let cube = bevy::prelude::Mesh::from(Cuboid::from_length(1.0));
        let red = [1.0, 0.0, 0.0, 1.0];
        let colored = MeshData::from_mesh(&cube, &Transform::default(), red);
        let mut plain = colored.clone();
        plain.colors.clear();

        let mut combined = colored.clone();
        combined.append(&plain);
        combined.append(&colored);

        let count = colored.positions.len();
        assert_eq!(combined.colors.len(), count * 3);
        assert!(combined.colors[..count].iter().all(|color| *color == red));
        assert!(combined.colors[count..count * 2].iter().all(|color| *color == [1.0; 4]));
        assert!(combined.colors[count * 2..].iter().all(|color| *color == red));
    }

    #[test]
    fn triangulates_concave_shapes_with_straight_runs() {
        // An L shape with extra points along its edges.
        let shape = [
            (0.0, 0.0),
            (1.0, 0.0),
            (2.0, 0.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (0.0, 2.0),
            (0.0, 1.0),
        ];
        let tris = super::triangulate(&shape);
        let area: f32 = tris
            .iter()
            .map(|t| {
                let p = |i: u32| shape[i as usize];
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)) / 2.0
            })
            .sum();
        assert!((area - 3.0).abs() < 1e-5, "covered {area}");
    }
}
