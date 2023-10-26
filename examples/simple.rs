use std::mem::size_of;
use xatlas_rs::*;

fn main() {
    let vertices = [
        [0.0, 0.0, 0.0], //
        [0.0, 1.0, 1.0], //
        [0.0, 1.0, 0.0],
    ];

    let indices = [0, 1, 2];

    let vertex_position_data = vertices
        .iter()
        .flatten()
        .flat_map(|v| (*v as f32).to_le_bytes())
        .collect::<Vec<_>>();

    let index_data = indices
        .iter()
        .flat_map(|i| (*i as u32).to_le_bytes())
        .collect::<Vec<_>>();

    let mesh = MeshDecl {
        vertex_count: vertices.len() as u32,
        vertex_position_data: &vertex_position_data,
        vertex_position_stride: (size_of::<f32>() * 3) as u32,
        index_count: (indices.len()) as u32,
        index_data: Some(&index_data),
        index_format: IndexFormat::UInt32,
        ..MeshDecl::default()
    };

    let mut atlas = Xatlas::new();
    atlas.add_mesh(&mesh).unwrap();

    atlas.generate(&Default::default(), &Default::default());

    let meshes = atlas.meshes();

    dbg!(meshes);
}
