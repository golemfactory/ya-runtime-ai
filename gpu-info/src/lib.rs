use model::{Cuda, Gpu};
use nvml_wrapper::{bitmasks::InitFlags, Device, Nvml};
use anyhow::{bail, Context, Result};

pub mod model;


/* testing
let nvml = Nvml::init()?;
// Get the first `Device` (GPU) in the system
log::info!("Cuda version: {}", nvml.sys_cuda_driver_version().expect("Can get CUDA version"));
match nvml.device_count() {
    Ok(count) => {
        for index in 0..count {
            match nvml.device_by_index(index) {
                Ok(dev) => {
                    log::info!("Device index: {index}");
                    log::info!("Device name: {}", dev.name().expect("Can get device name"));
                    log::info!("Device cores: {}", dev.num_cores().expect("Can get device cores"));
                    log::info!("Device mem info: {:?}", dev.memory_info().unwrap());
                    log::info!("Device cuda compute capability: {:?}", dev.cuda_compute_capability().unwrap());
                    log::info!("Device GPU clock info: {:?}", dev.clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics).unwrap());
                    log::info!("Device Memory clock info: {:?}", dev.clock_info(nvml_wrapper::enum_wrappers::device::Clock::Memory).unwrap());
                    log::info!("Device SM clock info: {:?}", dev.clock_info(nvml_wrapper::enum_wrappers::device::Clock::SM).unwrap());
                    log::info!("Device Video clock info: {:?}", dev.clock_info(nvml_wrapper::enum_wrappers::device::Clock::Video).unwrap());
                    log::info!("Device memory bus width: {:?}", dev.memory_bus_width().unwrap());
                    log::info!("Device memory clocks: {:?}", dev.supported_memory_clocks().unwrap());
                    // max supported_memory_clocks *2 == memTransferRatemax
                    
                },
                Err(err) => log::error!("Failed to get GPU {index} info. Err: {err}"),
            }
        }
    },
    Err(err) => log::error!("Failed to get GPU info. Err: {err}"),
}
*/

pub struct GpuInfo {
    nvml: Nvml,
}

impl GpuInfo {
    pub fn init() -> anyhow::Result<Self> {
        let nvml = Nvml::builder().flags(InitFlags::NO_ATTACH).init()?;
        return Ok(Self { nvml });
    }

    /// `uuid` of GPU device. If not provided first available GPU device will be used.
    pub fn info<S: AsRef<str>>(&self, uuid: Option<&str>) -> Result<GpuInfo> {
        if let Some(uuid) = uuid {
            let dev = self.nvml.device_by_uuid(uuid).with_context(|| format!("Failed to get GPU device with UUID: {uuid}."))?;
            return self.device_info(dev);
        };

        let gpu_count = self.nvml.device_count().with_context(|| "Unable to get count of CUDA devices.")?;
        if gpu_count == 0 {
            bail!("No supported GPU device available.")
        }

        let dev = self.nvml.device_by_index(0).with_context(|| "Failed to get GPU device.")?;
        self.device_info(dev)
    }

    fn device_info(&self, dev: Device) -> Result<Gpu> {
        let compute_capability = dev.cuda_compute_capability()?;
        todo!()
    }

    fn cuda(&self) -> Result<Cuda> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
