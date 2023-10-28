#![allow(unused)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use crate::root::xatlas;
use crate::root::xatlas::{IndexFormat_UInt16, IndexFormat_UInt32, ParameterizeFunc};
use std::ffi::c_void;
use std::marker::{PhantomData, PhantomPinned};
use std::ops::Deref;
use std::os::fd::AsFd;
use std::pin::Pin;
use std::ptr::slice_from_raw_parts;
use std::{mem, slice};

pub struct Xatlas<'x> {
    handle: *mut xatlas::Atlas,
    progress_callback: Option<ProgressCallbackPointer>,
    phantom: PhantomData<&'x ()>,
}

#[derive(Debug)]
pub enum MeshData<'a> {
    Contiguous(&'a [f32]),
    WithStride { data: &'a [u8], stride: u32 },
}

#[derive(Debug)]
pub enum IndexData<'a> {
    U16(&'a [u16]),
    U32(&'a [u32]),
}

#[derive(Debug)]
pub struct MeshDecl<'a> {
    pub vertex_position_data: MeshData<'a>,
    pub vertex_normal_data: Option<MeshData<'a>>,
    /// The input UVs are provided as a hint to the chart generator.
    pub vertex_uv_data: Option<MeshData<'a>>,

    pub face_ignore_data: Option<bool>,
    /// Must be faceCount in length.
    /// Only faces with the same material will be assigned to the same chart.
    pub face_material_data: Option<&'a [u32]>,

    /// Must be faceCount in length.
    /// Polygon / n-gon support. Faces are assumed to be triangles if this is null.
    pub face_vertex_count: Option<&'a [u8]>,

    pub index_data: Option<IndexData<'a>>,

    /// if faceVertexCount is null. Otherwise assumed to be indexCount / 3.
    pub face_count: u32,
    pub epsilon: f32,
}

pub struct UvMeshDecl<'a> {
    pub vertex_uv_data: Option<MeshData<'a>>,
    ///Overlapping UVs should be assigned a different material. Must be indexCount / 3 in length.
    pub face_material_data: Option<&'a [u32]>,
    pub index_data: Option<IndexData<'a>>,
}

#[derive(Debug)]
pub enum AddMeshError {
    /// Unspecified error.
    Error,
    /// An index is >= MeshDecl vertexCount.
    IndexOutOfRange,
    /// Must be >= 3.
    InvalidFaceVertexCount,
    /// Not evenly divisible by 3 - expecting triangles.
    InvalidIndexCount,
}

pub struct ChartOptions {
    pub param_func: ParameterizeFunc,

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
    pub max_chart_size: u32,

    /// Number of pixels to pad charts with.
    pub padding: u32,

    /// Unit to texel scale. e.g. a 1x1 quad with texels_per_unit of 32 will take up approximately 32x32 texels in the atlas.
    /// If 0, an estimated value will be calculated to approximately match the given resolution.
    /// If resolution is also 0, the estimated value will approximately match a 1024x1024 atlas.
    pub texels_per_unit: f32,

    /// If 0, generate a single atlas with texels_per_unit determining the final resolution.
    /// If not 0, and texels_per_unit is not 0, generate one or more atlases with that exact resolution.
    /// If not 0, and texels_per_unit is 0, texels_per_unit is estimated to approximately match the resolution.
    pub resolution: u32,

    /// Leave space around charts for texels that would be sampled by bilinear filtering.
    pub bilinear: bool,

    /// Align charts to 4x4 blocks. Also improves packing speed, since there are fewer possible chart locations to consider.
    pub block_align: bool,

    /// Slower, but gives the best result. If false, use random chart placement.
    pub brute_force: bool,

    /// Create Atlas::image
    pub create_image: bool,

    /// Rotate charts to the axis of their convex hull.
    pub rotate_charts_to_axis: bool,

    /// Rotate charts to improve packing.
    pub rotate_charts: bool,
}

#[derive(Debug)]
pub enum ProgressCategory {
    AddMesh,
    ComputeCharts,
    PackCharts,
    BuildOutputMeshes,
}

#[derive(Debug)]
pub struct Mesh<'a> {
    pub index_array: &'a [u32],
    pub chart_array: Vec<Chart<'a>>,
    pub vertex_array: Vec<Vertex>,
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
            progress_callback: None,
            phantom: PhantomData,
        }
    }

    pub fn width(&self) -> u32 {
        unsafe { *self.handle }.width
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

    pub fn utilization(&self) -> Option<&'x [f32]> {
        unsafe {
            if (*self.handle).utilization.is_null() {
                None
            } else {
                Some(slice::from_raw_parts(
                    (*self.handle).utilization,
                    (*self.handle).atlasCount as usize,
                ))
            }
        }
    }

    pub fn image(&self) -> Option<&'x [u32]> {
        unsafe {
            if (*self.handle).image.is_null() {
                None
            } else {
                Some(slice::from_raw_parts(
                    (*self.handle).image,
                    ((*self.handle).atlasCount * (*self.handle).width * (*self.handle).height)
                        as usize,
                ))
            }
        }
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
            vertexNormalData: match &mesh_decl.vertex_normal_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            vertexUvData: match &mesh_decl.vertex_uv_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            indexData: match &mesh_decl.index_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            faceIgnoreData: match mesh_decl.face_ignore_data {
                None => std::ptr::null(),
                Some(ref d) => d,
            },
            faceMaterialData: match mesh_decl.face_material_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr(),
            },
            faceVertexCount: match mesh_decl.face_vertex_count {
                None => std::ptr::null(),
                Some(d) => d.as_ptr(),
            },
            vertexCount: mesh_decl.vertex_position_data.count(),
            vertexPositionStride: mesh_decl.vertex_position_data.stride(3),
            vertexNormalStride: mesh_decl
                .vertex_normal_data
                .as_ref()
                .map_or(0, |c| c.stride(3)),
            vertexUvStride: mesh_decl.vertex_uv_data.as_ref().map_or(0, |d| d.stride(2)),
            indexCount: mesh_decl.index_data.as_ref().map_or(0, |d| d.count()),
            indexOffset: 0,
            faceCount: mesh_decl.face_count,
            indexFormat: mesh_decl
                .index_data
                .as_ref()
                .map_or(IndexFormat_UInt16, |d| match d {
                    IndexData::U16(_) => IndexFormat_UInt16,
                    IndexData::U32(_) => IndexFormat_UInt32,
                }),
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
            vertexUvData: match &decl.vertex_uv_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            indexData: match &decl.index_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr() as _,
            },
            faceMaterialData: match decl.face_material_data {
                None => std::ptr::null(),
                Some(d) => d.as_ptr(),
            },
            vertexCount: match &decl.vertex_uv_data {
                None => 0,
                Some(d) => d.count(),
            },
            vertexStride: match &decl.vertex_uv_data {
                None => 0,
                Some(d) => d.stride(2),
            },
            indexCount: match &decl.index_data {
                None => 0,
                Some(d) => d.count(),
            },
            indexOffset: 0,
            indexFormat: match decl.index_data {
                None => IndexFormat_UInt16,
                Some(ref d) => match d {
                    IndexData::U16(_) => IndexFormat_UInt16,
                    IndexData::U32(_) => IndexFormat_UInt32,
                },
            },
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

    pub fn set_progress_callback(
        &mut self,
        callback: impl Fn(ProgressCategory, i32) -> bool + 'static,
    ) {
        let callback: ProgressCallbackPointer = Box::pin(Box::new(callback));
        let user_data = &*callback as *const _ as *mut _;
        self.progress_callback = Some(callback);

        unsafe { xatlas::SetProgressCallback(self.handle, Some(progress_callback), user_data) }
    }
}

/// Callback type that fits inside of a *void. Note that a single Box would not fit
/// because it is 128 bits instead of 64
type ProgressCallbackPointer = Pin<Box<Box<dyn Fn(ProgressCategory, i32) -> bool>>>;

unsafe extern "C" fn progress_callback(
    category: xatlas::ProgressCategory,
    progress: std::os::raw::c_int,
    user_data: *mut std::os::raw::c_void,
) -> bool {
    let progress: i32 = progress;
    let category = match category {
        xatlas::ProgressCategory_AddMesh => ProgressCategory::AddMesh,
        xatlas::ProgressCategory_ComputeCharts => ProgressCategory::ComputeCharts,
        xatlas::ProgressCategory_PackCharts => ProgressCategory::PackCharts,
        xatlas::ProgressCategory_BuildOutputMeshes => ProgressCategory::BuildOutputMeshes,
        _ => unreachable!(),
    };

    let callback: ProgressCallbackPointer = unsafe { mem::transmute(user_data) };
    let result = callback(category, progress);
    mem::forget(callback);

    result
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

impl ChartOptions {
    fn convert(&self) -> xatlas::ChartOptions {
        xatlas::ChartOptions {
            paramFunc: self.param_func,
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

impl Default for Xatlas<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ChartOptions {
    fn default() -> Self {
        ChartOptions {
            param_func: None,
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
            vertex_position_data: MeshData::Contiguous(&[]),
            vertex_normal_data: None,
            vertex_uv_data: None,
            index_data: None,
            face_ignore_data: None,
            face_material_data: None,
            face_vertex_count: None,
            face_count: 0,
            epsilon: 1.1920929e-7f32,
        }
    }
}

impl MeshData<'_> {
    fn as_ptr(&self) -> *const u8 {
        match self {
            MeshData::Contiguous(d) => d.as_ptr() as _,
            MeshData::WithStride { data, .. } => data.as_ptr(),
        }
    }

    fn stride(&self, num: u32) -> u32 {
        match self {
            MeshData::Contiguous(_) => num * std::mem::size_of::<f32>() as u32,
            MeshData::WithStride { stride, .. } => *stride,
        }
    }

    fn count(&self) -> u32 {
        match self {
            MeshData::Contiguous(d) => (d.len() / 3) as u32,
            MeshData::WithStride { data, stride, .. } => data.len() as u32 / stride,
        }
    }
}

impl IndexData<'_> {
    fn as_ptr(&self) -> *const u8 {
        match self {
            IndexData::U16(d) => d.as_ptr() as _,
            IndexData::U32(d) => d.as_ptr() as _,
        }
    }

    fn count(&self) -> u32 {
        (match self {
            IndexData::U16(d) => d.len(),
            IndexData::U32(d) => d.len(),
        }) as _
    }
}

impl Drop for Xatlas<'_> {
    fn drop(&mut self) {
        unsafe {
            xatlas::Destroy(self.handle);
        }
    }
}
