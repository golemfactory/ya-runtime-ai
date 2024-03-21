use serde::Serialize;

#[serde(rename_all = "kebab-case")]
#[derive(Clone, Debug, Serialize)]
pub struct Gpu {
    pub model: String,
    pub cuda: Cuda,
    pub clocks: Clocks,
    pub memory: Memory,
}

#[derive(Clone, Debug, Serialize)]
pub struct Cuda {
    pub enabled: bool,
    pub cores: u32,
    pub version: String,
    pub compute_capability: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Clocks {
    #[serde(rename(serialize = "graphics.mhz"))]
    pub graphics_mhz: u32,
    #[serde(rename(serialize = "memory.mhz"))]
    pub memory_mhz: u32,
    #[serde(rename(serialize = "sm.mhz"))]
    pub sm_mhz: u32,
    #[serde(rename(serialize = "video.mhz"))]
    pub video_mhz: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Memory {
    #[serde(rename(serialize = "bandwidth.gib"))]
    pub bandwidth_gib: u32,
    #[serde(rename(serialize = "tatal.gib"))]
    pub total_gib: f32
}
