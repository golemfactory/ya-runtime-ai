use anyhow::{bail, Context};
use model::{Clocks, Cuda, Gpu, Memory};
use nvml_wrapper::{enum_wrappers::device::Clock, Device, Nvml};

pub mod model;

pub struct GpuInfo {
    nvml: Nvml,
}

impl GpuInfo {
    pub fn init() -> anyhow::Result<Self> {
        let nvml = Nvml::init()?;
        Ok(Self { nvml })
    }

    /// `uuid` of GPU device. If not provided first available GPU device will be used.
    pub fn info<S: AsRef<str>>(&self, uuid: Option<S>) -> anyhow::Result<Gpu> {
        if let Some(uuid) = uuid {
            let dev = self.nvml.device_by_uuid(uuid.as_ref()).with_context(|| {
                format!("Failed to get GPU device with UUID: {}.", uuid.as_ref())
            })?;
            return self.device_info(dev);
        };

        let gpu_count = self
            .nvml
            .device_count()
            .context("Unable to get count of CUDA devices.")?;

        if gpu_count == 0 {
            bail!("No supported GPU device available.")
        }

        let dev = self
            .nvml
            .device_by_index(0)
            .context("Failed to get GPU device.")?;

        self.device_info(dev)
    }

    fn device_info(&self, dev: Device) -> anyhow::Result<Gpu> {
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

    fn cuda_version(&self) -> anyhow::Result<String> {
        let version = self
            .nvml
            .sys_cuda_driver_version()
            .context("Unable to get driver version")?;
        let version_major = nvml_wrapper::cuda_driver_version_major(version);
        let version_minor = nvml_wrapper::cuda_driver_version_minor(version);
        Ok(format!("{}.{}", version_major, version_minor))
    }
}

fn cuda(dev: &Device, version: String) -> anyhow::Result<Cuda> {
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

fn compute_capability(dev: &Device) -> anyhow::Result<String> {
    let capability = dev.cuda_compute_capability()?;
    Ok(format!("{}.{}", capability.major, capability.minor))
}

fn clocks(dev: &Device) -> anyhow::Result<Clocks> {
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

fn memory(dev: &Device) -> anyhow::Result<Memory> {
    let total_bytes = dev.memory_info()?.total;
    let total_gib = bytes_to_gib(total_bytes);
    let bandwidth_gib = bandwidth_gib(dev)?;
    Ok(Memory {
        bandwidth_gib,
        total_gib,
    })
}

fn bandwidth_gib(dev: &Device) -> anyhow::Result<u32> {
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
