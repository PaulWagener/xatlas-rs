use xatlas_rs::*;

fn main() {
    let vertices = [
        0.0, 0.0, 0.0, //
        0.0, 1.0, 1.0, //
        0.0, 1.0, 0.0,
    ];

    let indices = [0, 1, 2];

    let mesh = MeshDecl {
        vertex_position_data: MeshData::Contiguous(&vertices),
        index_data: Some(IndexData::U32(&indices)),
        ..MeshDecl::default()
    };

    let mut atlas = Xatlas::new();
    atlas.set_progress_callback(move |category, progress| {
        println!("Progress: {category:?} {progress}");
        true
    });
    atlas.add_mesh(&mesh).unwrap();
    atlas.generate(&Default::default(), &Default::default());
    let meshes = atlas.meshes();

    dbg!(meshes);
}
