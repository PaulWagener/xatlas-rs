#![allow(unused)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use crate::root::xatlas;
use std::marker::{PhantomData, PhantomPinned};
use std::os::fd::AsFd;
use std::ptr::slice_from_raw_parts;
use std::slice;

#[derive(Debug)]
pub struct Xatlas<'x> {
    handle: *mut xatlas::Atlas,
    phantom: PhantomData<&'x ()>,
}

#[derive(Debug, Default)]
pub enum IndexFormat {
    #[default]
    UInt16,
    UInt32,
}

#[derive(Debug)]
pub struct MeshDecl<'a> {
    pub vertex_position_data: &'a [u8],
    pub vertex_normal_data: Option<&'a [u8]>,
    pub vertex_uv_data: Option<&'a [u8]>,
    pub index_data: Option<&'a [u8]>,
    pub face_ignore_data: Option<bool>,
    pub face_material_data: Option<u32>,
    pub face_vertex_count: Option<u8>,
    pub vertex_count: u32,
    pub vertex_position_stride: u32,
    pub vertex_normal_stride: u32,
    pub vertex_uv_stride: u32,
    pub index_count: u32,
    pub index_offset: i32,
    pub face_count: u32,
    pub index_format: IndexFormat,
    pub epsilon: f32,
}

pub struct UvMeshDecl<'a> {
    pub vertex_uv_data: &'a [u8],
    pub index_data: Option<&'a [u8]>,
    pub face_material_data: Option<u32>,

    pub vertex_count: u32,
    pub vertex_stride: u32,
    pub index_count: u32,
    pub index_offset: i32,
    pub index_format: IndexFormat,
}

#[derive(Debug)]
pub enum AddMeshError {
    Error,                  // Unspecified error.
    IndexOutOfRange,        // An index is >= MeshDecl vertexCount.
    InvalidFaceVertexCount, // Must be >= 3.
    InvalidIndexCount,      // Not evenly divisible by 3 - expecting triangles.
}

pub struct ChartOptions {
    /// Don't grow charts to be larger than this. 0 means no limit.
    pub max_chart_area: f32,

    /// Don't grow charts to have a longer boundary than this. 0 means no limit.
    pub max_boundary_length: f32,

    /// Weights determine chart growth. Higher weights mean higher cost for that metric.
    /// Angle between face and average chart normal.
    pub normal_deviation_weight: f32,
    pub roundness_weight: f32,
    pub straightness_weight: f32,

    /// If > 1000, normal seams are fully respected.
    pub normal_seam_weight: f32,
    pub texture_seam_weight: f32,

    /// If total of all metrics * weights > max_cost, don't grow chart. Lower values result in more charts.
    pub max_cost: f32,
    /// Number of iterations of the chart growing and seeding phases. Higher values result in better charts.
    pub max_iterations: u32,

    /// Use MeshDecl::vertex_uv_data for charts.
    pub use_input_mesh_uvs: bool,
    /// Enforce consistent texture coordinate winding.
    pub fix_winding: bool,
}

pub struct PackOptions {
    /// Charts larger than this will be scaled down. 0 means no limit.
    max_chart_size: u32,

    /// Number of pixels to pad charts with.
    padding: u32,

    /// Unit to texel scale. e.g. a 1x1 quad with texels_per_unit of 32 will take up approximately 32x32 texels in the atlas.
    /// If 0, an estimated value will be calculated to approximately match the given resolution.
    /// If resolution is also 0, the estimated value will approximately match a 1024x1024 atlas.
    texels_per_unit: f32,

    /// If 0, generate a single atlas with texels_per_unit determining the final resolution.
    /// If not 0, and texels_per_unit is not 0, generate one or more atlases with that exact resolution.
    /// If not 0, and texels_per_unit is 0, texels_per_unit is estimated to approximately match the resolution.
    resolution: u32,

    /// Leave space around charts for texels that would be sampled by bilinear filtering.
    bilinear: bool,

    /// Align charts to 4x4 blocks. Also improves packing speed, since there are fewer possible chart locations to consider.
    block_align: bool,

    /// Slower, but gives the best result. If false, use random chart placement.
    brute_force: bool,

    /// Create Atlas::image
    create_image: bool,

    /// Rotate charts to the axis of their convex hull.
    rotate_charts_to_axis: bool,

    /// Rotate charts to improve packing.
    rotate_charts: bool,
}

pub enum ProgressCategory {
    AddMesh,
    ComputeCharts,
    PackCharts,
    BuildOutputMeshes,
}

#[derive(Debug)]
pub struct Mesh<'a> {
    index_array: &'a [u32],
    chart_array: Vec<Chart<'a>>,
    vertex_array: Vec<Vertex>,
}

#[derive(Debug)]
pub struct Vertex {
    pub atlas_index: i32,
    pub chart_index: i32,
    pub uv: [f32; 2usize],
    pub xref: u32,
}

#[derive(Debug)]
pub struct Chart<'a> {
    pub face_array: &'a [u32],
    pub atlas_index: u32,
    pub type_: ChartType,
    pub material: u32,
}

#[derive(Debug)]
pub enum ChartType {
    Planar,
    Ortho,
    LSCM,
    Piecewise,
    Invalid,
}

impl<'x> Xatlas<'x> {
    pub fn new() -> Self {
        Self {
            handle: unsafe { xatlas::Create() },
            phantom: PhantomData::default(),
        }
    }

    pub fn width(&self) -> u32 {
        unsafe { *self.handle }.height
    }

    pub fn height(&self) -> u32 {
        unsafe { *self.handle }.height
    }

    pub fn atlas_count(&self) -> u32 {
        unsafe { *self.handle }.atlasCount
    }

    pub fn chart_count(&self) -> u32 {
        unsafe { *self.handle }.chartCount
    }

    pub fn mesh_count(&self) -> u32 {
        unsafe { *self.handle }.meshCount
    }

    pub fn texels_per_unit(&self) -> f32 {
        unsafe { *self.handle }.texelsPerUnit
    }

    pub fn meshes(&self) -> Vec<Mesh<'x>> {
        unsafe { slice::from_raw_parts((*self.handle).meshes, (*self.handle).meshCount as usize) }
            .iter()
            .map(|mesh| {
                let chart_array =
                    unsafe { slice::from_raw_parts(mesh.chartArray, mesh.chartCount as usize) }
                        .iter()
                        .map(|chart| Chart {
                            face_array: unsafe {
                                slice::from_raw_parts(chart.faceArray, chart.faceCount as usize)
                            },
                            atlas_index: chart.atlasIndex,
                            type_: match chart.type_ {
                                xatlas::ChartType_Planar => ChartType::Planar,
                                xatlas::ChartType_Ortho => ChartType::Ortho,
                                xatlas::ChartType_LSCM => ChartType::LSCM,
                                xatlas::ChartType_Piecewise => ChartType::Piecewise,
                                xatlas::ChartType_Invalid => ChartType::Invalid,
                                _ => unreachable!(),
                            },
                            material: chart.material,
                        })
                        .collect();

                let vertex_array =
                    unsafe { slice::from_raw_parts(mesh.vertexArray, mesh.vertexCount as usize) }
                        .iter()
                        .map(|vertex| Vertex {
                            atlas_index: vertex.atlasIndex,
                            chart_index: vertex.chartIndex,
                            uv: vertex.uv,
                            xref: vertex.xref,
                        })
                        .collect();

                let index_array =
                    unsafe { slice::from_raw_parts(mesh.indexArray, mesh.indexCount as usize) };

                Mesh {
                    index_array,
                    chart_array,
                    vertex_array,
                }
            })
            .collect()
    }

    pub fn add_mesh(&mut self, mesh_decl: &MeshDecl<'x>) -> Result<(), AddMeshError> {
        self.add_mesh_with_mesh_count_hint(mesh_decl, 0)
    }

    pub fn add_mesh_with_mesh_count_hint(
        &mut self,
        mesh_decl: &MeshDecl<'x>,
        mesh_count_hint: u32,
    ) -> Result<(), AddMeshError> {
        let decl = xatlas::MeshDecl {
            vertexPositionData: mesh_decl.vertex_position_data.as_ptr() as _,
            vertexNormalData: match mesh_decl.vertex_normal_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            vertexUvData: match mesh_decl.vertex_uv_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            indexData: match mesh_decl.index_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            faceIgnoreData: match mesh_decl.face_ignore_data {
                None => std::ptr::null(),
                Some(ref d) => d,
            },
            faceMaterialData: match mesh_decl.face_material_data {
                None => std::ptr::null(),
                Some(ref d) => d,
            },
            faceVertexCount: match mesh_decl.face_vertex_count {
                None => std::ptr::null(),
                Some(ref d) => d,
            },
            vertexCount: mesh_decl.vertex_count,
            vertexPositionStride: mesh_decl.vertex_position_stride,
            vertexNormalStride: mesh_decl.vertex_normal_stride,
            vertexUvStride: mesh_decl.vertex_uv_stride,
            indexCount: mesh_decl.index_count,
            indexOffset: mesh_decl.index_offset,
            faceCount: mesh_decl.face_count,
            indexFormat: convert_index_format(&mesh_decl.index_format),
            epsilon: mesh_decl.epsilon,
        };

        let result = unsafe { xatlas::AddMesh(self.handle, &decl, mesh_count_hint) };

        add_mesh_error_result(result)
    }

    pub fn add_mesh_join(&mut self) {
        unsafe { xatlas::AddMeshJoin(self.handle) }
    }

    pub fn add_uv_mesh(&mut self, decl: &UvMeshDecl<'x>) -> Result<(), AddMeshError> {
        let decl = xatlas::UvMeshDecl {
            vertexUvData: decl.vertex_uv_data.as_ptr() as _,
            indexData: match decl.index_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            faceMaterialData: match decl.face_material_data {
                None => std::ptr::null(),
                Some(ref d) => d,
            },
            vertexCount: decl.vertex_count,
            vertexStride: decl.vertex_stride,
            indexCount: decl.index_count,
            indexOffset: decl.index_offset,
            indexFormat: convert_index_format(&decl.index_format),
        };
        let result = unsafe { xatlas::AddUvMesh(self.handle, &decl) };

        add_mesh_error_result(result)
    }

    /// Call after all AddMesh calls. Can be called multiple times to recompute charts with different options.
    pub fn compute_charts(&mut self, options: &ChartOptions) {
        let options = options.convert();

        unsafe { xatlas::ComputeCharts(self.handle, options) }
    }

    /// Call after ComputeCharts. Can be called multiple times to re-pack charts with different options.
    pub fn pack_charts(&mut self, pack_options: &PackOptions) {
        let pack_options = pack_options.convert();

        unsafe { xatlas::PackCharts(self.handle, pack_options) }
    }

    /// Equivalent to calling ComputeCharts and PackCharts in sequence. Can be called multiple times to regenerate with different options.
    pub fn generate(&mut self, chart_options: &ChartOptions, pack_options: &PackOptions) {
        let chart_options = chart_options.convert();
        let pack_options = pack_options.convert();

        unsafe { xatlas::Generate(self.handle, chart_options, pack_options) }
    }
}

fn add_mesh_error_result(add_mesh_error: xatlas::AddMeshError) -> Result<(), AddMeshError> {
    match add_mesh_error {
        xatlas::AddMeshError_Success => Ok(()),
        xatlas::AddMeshError_Error => Err(AddMeshError::Error),
        xatlas::AddMeshError_IndexOutOfRange => Err(AddMeshError::IndexOutOfRange),
        xatlas::AddMeshError_InvalidFaceVertexCount => Err(AddMeshError::InvalidFaceVertexCount),
        xatlas::AddMeshError_InvalidIndexCount => Err(AddMeshError::InvalidIndexCount),
        _ => unreachable!(),
    }
}

fn convert_index_format(index_format: &IndexFormat) -> xatlas::IndexFormat {
    match index_format {
        IndexFormat::UInt16 => xatlas::IndexFormat_UInt16,
        IndexFormat::UInt32 => xatlas::IndexFormat_UInt32,
    }
}

impl ChartOptions {
    fn convert(&self) -> xatlas::ChartOptions {
        xatlas::ChartOptions {
            paramFunc: None,
            maxChartArea: self.max_chart_area,
            maxBoundaryLength: self.max_boundary_length,
            normalDeviationWeight: self.normal_deviation_weight,
            roundnessWeight: self.roundness_weight,
            straightnessWeight: self.straightness_weight,
            normalSeamWeight: self.normal_seam_weight,
            textureSeamWeight: self.texture_seam_weight,
            maxCost: self.max_cost,
            maxIterations: self.max_iterations,
            useInputMeshUvs: self.use_input_mesh_uvs,
            fixWinding: self.fix_winding,
        }
    }
}

impl Default for ChartOptions {
    fn default() -> Self {
        ChartOptions {
            max_chart_area: 0.0,
            max_boundary_length: 0.0,
            normal_deviation_weight: 2.0,
            roundness_weight: 0.01,
            straightness_weight: 6.0,
            normal_seam_weight: 4.0,
            texture_seam_weight: 0.5,
            max_cost: 2.0,
            max_iterations: 1,
            use_input_mesh_uvs: false,
            fix_winding: false,
        }
    }
}

impl PackOptions {
    fn convert(&self) -> xatlas::PackOptions {
        xatlas::PackOptions {
            maxChartSize: self.max_chart_size,
            padding: self.padding,
            texelsPerUnit: self.texels_per_unit,
            resolution: self.resolution,
            bilinear: self.bilinear,
            blockAlign: self.block_align,
            bruteForce: self.brute_force,
            createImage: self.create_image,
            rotateChartsToAxis: self.rotate_charts_to_axis,
            rotateCharts: self.rotate_charts,
        }
    }
}

impl Default for PackOptions {
    fn default() -> Self {
        PackOptions {
            max_chart_size: 0,
            padding: 0,
            texels_per_unit: 0.0,
            resolution: 0,
            bilinear: true,
            block_align: false,
            brute_force: false,
            create_image: false,
            rotate_charts_to_axis: true,
            rotate_charts: true,
        }
    }
}

impl Default for MeshDecl<'_> {
    fn default() -> Self {
        MeshDecl {
            vertex_position_data: &[],
            vertex_normal_data: None,
            vertex_uv_data: None,
            index_data: None,
            face_ignore_data: None,
            face_material_data: None,
            face_vertex_count: None,
            vertex_count: 0,
            vertex_position_stride: 0,
            vertex_normal_stride: 0,
            vertex_uv_stride: 0,
            index_count: 0,
            index_offset: 0,
            face_count: 0,
            index_format: Default::default(),
            epsilon: 1.192092896e-07f32,
        }
    }
}

impl Drop for Xatlas<'_> {
    fn drop(&mut self) {
        unsafe {
            xatlas::Destroy(self.handle);
        }
    }
}
