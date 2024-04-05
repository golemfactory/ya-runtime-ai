use model::{Clocks, Cuda, Gpu, Memory};
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::{enum_wrappers::device::Clock, Device, Nvml};
use thiserror::Error;

pub mod model;

#[derive(Error, Debug)]
pub enum GpuDetectionError {
    #[error("libloading error occurred: {0}")]
    LibloadingError(#[from] libloading::Error),
    #[error("Failed to access GPU error: {0}")]
    GpuAccessError(String),
    #[error("Failed to access GPU info error: {0}")]
    GpuInfoAccessError(String),
    #[error("NVML error occurred: {0}")]
    Unknown(String),
}

pub struct GpuDetection {
    nvml: Nvml,
}

impl GpuDetection {
    pub fn init() -> Result<Self, GpuDetectionError> {
        let nvml = match Nvml::init() {
            Ok(nvlm) => nvlm,
            Err(NvmlError::LibloadingError(e)) => {
                return Err(GpuDetectionError::LibloadingError(e))
            }
            Err(e) => return Err(GpuDetectionError::Unknown(e.to_string())),
        };
        Ok(Self { nvml })
    }

    /// `uuid` of GPU device. If not provided first available GPU device will be used.
    pub fn detect<S: AsRef<str>>(&self, uuid: Option<S>) -> Result<Gpu, GpuDetectionError> {
        if let Some(uuid) = uuid {
            let dev = self.nvml.device_by_uuid(uuid.as_ref()).map_err(|err| {
                GpuDetectionError::GpuAccessError(format!(
                    "Failed to get GPU device with UUID: {}. Err {}",
                    uuid.as_ref(),
                    err
                ))
            })?;
            return self
                .device_info(dev)
                .map_err(|err| GpuDetectionError::GpuInfoAccessError(err.to_string()));
        };

        let gpu_count = self.nvml.device_count().map_err(|err| {
            GpuDetectionError::Unknown(format!("Failed to get device count. Err {}", err))
        })?;

        if gpu_count == 0 {
            return Err(GpuDetectionError::GpuAccessError("No GPU available".into()));
        }

        let index = 0;
        let dev = self.nvml.device_by_index(index).map_err(|err| {
            GpuDetectionError::GpuAccessError(format!(
                "Failed to get GPU device under index: {}. Err {}",
                index, err
            ))
        })?;

        self.device_info(dev)
            .map_err(|err| GpuDetectionError::GpuInfoAccessError(err.to_string()))
    }

    fn device_info(&self, dev: Device) -> Result<Gpu, NvmlError> {
        let model = dev.name()?;
        let version = self.cuda_version()?;
        let cuda = cuda(&dev, version)?;
        let clocks = clocks(&dev)?;
        let memory = memory(&dev)?;
        Ok(Gpu {
            model,
            cuda,
            clocks,
            memory,
        })
    }

    fn cuda_version(&self) -> Result<String, NvmlError> {
        let version = self.nvml.sys_cuda_driver_version()?;
        let version_major = nvml_wrapper::cuda_driver_version_major(version);
        let version_minor = nvml_wrapper::cuda_driver_version_minor(version);
        Ok(format!("{}.{}", version_major, version_minor))
    }
}

fn cuda(dev: &Device, version: String) -> Result<Cuda, NvmlError> {
    let enabled = true;
    let cores = dev.num_cores()?;
    let compute_capability = compute_capability(dev)?;
    Ok(Cuda {
        enabled,
        cores,
        version,
        compute_capability,
    })
}

fn compute_capability(dev: &Device) -> Result<String, NvmlError> {
    let capability = dev.cuda_compute_capability()?;
    Ok(format!("{}.{}", capability.major, capability.minor))
}

fn clocks(dev: &Device) -> Result<Clocks, NvmlError> {
    let graphics_mhz = dev.max_clock_info(Clock::Graphics)?;
    let memory_mhz = dev.max_clock_info(Clock::Memory)?;
    let sm_mhz = dev.max_clock_info(Clock::SM)?;
    let video_mhz = dev.max_clock_info(Clock::Video)?;
    Ok(Clocks {
        graphics_mhz,
        memory_mhz,
        sm_mhz,
        video_mhz,
    })
}

fn memory(dev: &Device) -> Result<Memory, NvmlError> {
    let total_bytes = dev.memory_info()?.total;
    let total_gib = bytes_to_gib(total_bytes);
    Ok(Memory {
        bandwidth_gib: None,
        total_gib,
    })
}

/// Unused because of lack of `memTransferRatemax` property.
#[allow(dead_code)]
fn bandwidth_gib(dev: &Device) -> Result<u32, NvmlError> {
    let memory_bus_width = dev.memory_bus_width()?;
    let supported_memory_clocks = dev.supported_memory_clocks()?;
    let max_memory_clock = supported_memory_clocks.iter().cloned().fold(0, u32::max);
    // `nvml` does not provide `memTransferRatemax` like `nvidia-settings` tool does.
    // Transfer rate is a result of memory clock, bus width, and memory specific multiplier (for DDR it is 2)
    let data_rate = 2; // value for DDR
    let bandwidth_gib = max_memory_clock * memory_bus_width * data_rate / (1000 * 8);
    Ok(bandwidth_gib)
}

fn bytes_to_gib(memory: u64) -> f32 {
    (memory as f64 / 1024.0 / 1024.0 / 1024.0) as f32
}
