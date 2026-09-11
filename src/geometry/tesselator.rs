use crate::geometry::{
    mesh::{RawMesh, Vertex},
    shape::{Shape, UvRect},
};

pub struct Tesselator;

impl Tesselator {
    pub fn tesselate(shape: &Shape, mesh: &mut RawMesh) {
        match shape {
            Shape::Line {
                start,
                end,
                thickness,
                color,
            } => {
                let dx = end[0] - start[0];
                let dy = end[1] - start[1];

                let length = (dx * dx + dy * dy).sqrt();

                if length < 0.0001 {
                    return;
                }

                let half_thick = thickness * 0.5;
                let nx = (-dy / length) * half_thick;
                let ny = (dx / length) * half_thick;

                let p0 = [start[0] + nx, start[1] + ny];
                let p1 = [end[0] + nx, end[1] + ny];
                let p2 = [start[0] - nx, start[1] - ny];
                let p3 = [end[0] - nx, end[1] - ny];

                let i0 = mesh.push_vertex(Vertex {
                    pos: p0,
                    uv: [0.0, 0.0],
                    color: *color,
                });
                let i1 = mesh.push_vertex(Vertex {
                    pos: p1,
                    uv: [0.0, 0.0],
                    color: *color,
                });
                let i2 = mesh.push_vertex(Vertex {
                    pos: p2,
                    uv: [0.0, 0.0],
                    color: *color,
                });
                let i3 = mesh.push_vertex(Vertex {
                    pos: p3,
                    uv: [0.0, 0.0],
                    color: *color,
                });

                mesh.push_triangle(i0, i1, i2);
                mesh.push_triangle(i1, i3, i2);
            }
            Shape::Polygon {
                center,
                sides,
                radius,
                color,
            } => {
                let cx = center[0];
                let cy = center[1];

                let ci = mesh.push_vertex(Vertex {
                    pos: [cx, cy],
                    uv: [0.0, 0.0],
                    color: *color,
                });

                let mut rim = Vec::with_capacity(*sides as usize + 1);
                for i in 0..*sides {
                    let angle = i as f32 * (2.0 * std::f32::consts::PI / *sides as f32);
                    let x = cx + radius * angle.cos();
                    let y = cy + radius * angle.sin();
                    rim.push(mesh.push_vertex(Vertex {
                        pos: [x, y],
                        uv: [0.0, 0.0],
                        color: *color,
                    }));
                }

                for i in 0..*sides as usize {
                    let next = (i + 1) % *sides as usize;
                    mesh.push_triangle(ci, rim[i], rim[next]);
                }
            }
            Shape::Ring {
                center,
                inner_r,
                outer_r,
                start_angle,
                end_angle,
                resolution,
                color,
            } => {
                for i in 0..*resolution {
                    let t0 = i as f32 / *resolution as f32;
                    let t1 = (i + 1) as f32 / *resolution as f32;

                    let a0 = start_angle + t0 * (end_angle - start_angle);
                    let a1 = start_angle + t1 * (end_angle - start_angle);

                    let inner0 = [
                        center[0] + inner_r * a0.cos(),
                        center[1] + inner_r * a0.sin(),
                    ];
                    let outer0 = [
                        center[0] + outer_r * a0.cos(),
                        center[1] + outer_r * a0.sin(),
                    ];
                    let inner1 = [
                        center[0] + inner_r * a1.cos(),
                        center[1] + inner_r * a1.sin(),
                    ];
                    let outer1 = [
                        center[0] + outer_r * a1.cos(),
                        center[1] + outer_r * a1.sin(),
                    ];

                    let i0 = mesh.push_vertex(Vertex {
                        pos: inner0,
                        uv: [0.0, 0.0],
                        color: *color,
                    });
                    let i1 = mesh.push_vertex(Vertex {
                        pos: inner1,
                        uv: [0.0, 0.0],
                        color: *color,
                    });
                    let i2 = mesh.push_vertex(Vertex {
                        pos: outer0,
                        uv: [0.0, 0.0],
                        color: *color,
                    });
                    let i3 = mesh.push_vertex(Vertex {
                        pos: outer1,
                        uv: [0.0, 0.0],
                        color: *color,
                    });

                    mesh.push_triangle(i0, i1, i2);
                    mesh.push_triangle(i1, i3, i2);
                }
            }
            Shape::SlantedQuad {
                pos,
                size,
                skew,
                color,
            } => {
                let x0 = pos[0];
                let y0 = pos[1];
                let x1 = pos[0] + size[0];
                let y1 = pos[1] + size[1];

                let i0 = mesh.push_vertex(Vertex {
                    pos: [x0 + *skew, y0],
                    uv: [0.0, 0.0],
                    color: *color,
                });
                let i1 = mesh.push_vertex(Vertex {
                    pos: [x1 + skew, y0],
                    uv: [0.0, 0.0],
                    color: *color,
                });
                let i2 = mesh.push_vertex(Vertex {
                    pos: [x0, y1],
                    uv: [0.0, 0.0],
                    color: *color,
                });
                let i3 = mesh.push_vertex(Vertex {
                    pos: [x1, y1],
                    uv: [0.0, 0.0],
                    color: *color,
                });

                mesh.push_triangle(i0, i1, i2);
                mesh.push_triangle(i1, i3, i2);
            }
            Shape::RoundedRect {
                pos,
                size,
                radius,
                segments,
                color,
            } => {
                let x0 = pos[0];
                let y0 = pos[1];
                let x1 = pos[0] + size[0];
                let y1 = pos[1] + size[1];

                let corners = [
                    [x0 + radius, y0 + radius], // haut-gauche
                    [x1 - radius, y0 + radius], // haut-droit
                    [x1 - radius, y1 - radius], // bas-droit
                    [x0 + radius, y1 - radius], // bas-gauche
                ];

                let corner_angles = [
                    std::f32::consts::PI,             // haut-gauche : 180°
                    3.0 * std::f32::consts::PI / 2.0, // haut-droit  : 270°
                    0.0,                              // bas-droit   : 0°
                    std::f32::consts::PI / 2.0,       // bas-gauche  : 90°
                ];

                let step = std::f32::consts::PI / 2.0 / *segments as f32;

                let cx = (x0 + x1) * 0.5;
                let cy = (y0 + y1) * 0.5;

                let center_idx = mesh.push_vertex(Vertex {
                    pos: [cx, cy],
                    uv: [0.0, 0.0],
                    color: *color,
                });

                let mut prev_idx: Option<u32> = None;
                let mut first_idx: Option<u32> = None;

                for c in 0..4 {
                    let [ccx, ccy] = corners[c];
                    let base_angle = corner_angles[c];

                    for s in 0..=*segments {
                        let angle = base_angle + s as f32 * step;
                        let x = ccx + radius * angle.cos();
                        let y = ccy + radius * angle.sin();

                        let idx = mesh.push_vertex(Vertex {
                            pos: [x, y],
                            uv: [0.0, 0.0],
                            color: *color,
                        });

                        if first_idx.is_none() {
                            first_idx = Some(idx);
                        }

                        if let Some(prev) = prev_idx {
                            mesh.push_triangle(center_idx, prev, idx);
                        }

                        prev_idx = Some(idx);
                    }
                }
                if let (Some(prev), Some(first)) = (prev_idx, first_idx) {
                    mesh.push_triangle(center_idx, prev, first);
                }
            }
            Shape::Quad {
                pos,
                size,
                rotation,
                color,
                uv,
            } => {
                let uv_rect = uv.unwrap_or(UvRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                });

                let u0 = uv_rect.x;
                let v0 = uv_rect.y;
                let u1 = uv_rect.x + uv_rect.w;
                let v1 = uv_rect.y + uv_rect.h;

                let half_w = size[0] * 0.5;
                let half_h = size[1] * 0.5;

                let cx = pos[0] + half_w;
                let cy = pos[1] + half_h;

                let mut local_pts = [
                    [-half_w, -half_h], // Haut-Gauche
                    [half_w, -half_h],  // Haut-Droit
                    [-half_w, half_h],  // Bas-Gauche
                    [half_w, half_h],   // Bas-Droit
                ];

                if *rotation != 0.0 {
                    let cos = rotation.cos();
                    let sin = rotation.sin();

                    for pt in &mut local_pts {
                        let x = pt[0];
                        let y = pt[1];

                        pt[0] = x * cos - y * sin;
                        pt[1] = x * sin + y * cos;
                    }
                }

                let i0 = mesh.push_vertex(Vertex {
                    pos: [cx + local_pts[0][0], cy + local_pts[0][1]],
                    uv: [u0, v0],
                    color: *color,
                });
                let i1 = mesh.push_vertex(Vertex {
                    pos: [cx + local_pts[1][0], cy + local_pts[1][1]],
                    uv: [u1, v0],
                    color: *color,
                });
                let i2 = mesh.push_vertex(Vertex {
                    pos: [cx + local_pts[2][0], cy + local_pts[2][1]],
                    uv: [u0, v1],
                    color: *color,
                });
                let i3 = mesh.push_vertex(Vertex {
                    pos: [cx + local_pts[3][0], cy + local_pts[3][1]],
                    uv: [u1, v1],
                    color: *color,
                });
                mesh.push_triangle(i0, i1, i2);
                mesh.push_triangle(i1, i3, i2);
            }
            Shape::NinePatch {
                pos,
                size,
                texture_size,
                margins,
                color,
            } => {
                // Éviter les divisions par zéro si la taille de texture est invalide
                if texture_size[0] <= 0.0 || texture_size[1] <= 0.0 {
                    return;
                }

                // Coordonnées écran (X et Y)
                let x = [
                    pos[0],
                    pos[0] + margins.left,
                    pos[0] + size[0] - margins.right,
                    pos[0] + size[0],
                ];

                let y = [
                    pos[1],
                    pos[1] + margins.top,
                    pos[1] + size[1] - margins.bottom,
                    pos[1] + size[1],
                ];

                // Coordonnées UV normalisées [0.0..1.0]
                let u = [
                    0.0,
                    margins.left / texture_size[0],
                    1.0 - (margins.right / texture_size[0]),
                    1.0,
                ];

                let v = [
                    0.0,
                    margins.top / texture_size[1],
                    1.0 - (margins.bottom / texture_size[1]),
                    1.0,
                ];

                // Génération des 9 quads (3x3)
                for row in 0..3 {
                    for col in 0..3 {
                        let cell_w = x[col + 1] - x[col];
                        let cell_h = y[row + 1] - y[row];

                        // Ignore les cellules si la taille cible est trop petite pour afficher les marges
                        if cell_w <= 0.0 || cell_h <= 0.0 {
                            continue;
                        }

                        let cell_uv_w = u[col + 1] - u[col];
                        let cell_uv_h = v[row + 1] - v[row];

                        let quad = Shape::Quad {
                            pos: [x[col], y[row]],
                            size: [cell_w, cell_h],
                            rotation: 0.0,
                            color: *color,
                            uv: Some(UvRect {
                                x: u[col],
                                y: v[row],
                                w: cell_uv_w,
                                h: cell_uv_h,
                            }),
                        };

                        Self::tesselate(&quad, mesh);
                    }
                }
            }
        }
    }
}
