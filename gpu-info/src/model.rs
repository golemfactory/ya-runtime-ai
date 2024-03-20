use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Gpu {
    model: String,
    cuda: Cuda,
    clocks: Clocks,
    memory: Memory,
}

#[derive(Clone, Debug, Serialize)]
pub struct Cuda {
    enabled: bool,
    cores: u32,
    version: String,
    capability: Capability,
}

#[derive(Clone, Debug, Serialize)]
pub struct Capability {
    major: u32,
    minor: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Clocks {
    #[serde(rename(deserialize = "graphics.mhz"))]
    graphics_mhz: u32,
    #[serde(rename(deserialize = "memory.mhz"))]
    memory_mhz: u32,
    #[serde(rename(deserialize = "sm.mhz"))]
    sm_mhz: u32,
    #[serde(rename(deserialize = "video.mhz"))]
    video_mhz: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Memory {
    #[serde(rename(deserialize = "bandwidth.gib"))]
    bandwidth_gib: u32,
    #[serde(rename(deserialize = "tatal.gib"))]
    tatal_gib: u32
}
