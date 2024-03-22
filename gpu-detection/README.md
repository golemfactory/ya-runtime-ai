# gpu-detection

Library detects GPU info listed in [GAP-35](https://github.com/golemfactory/golem-architecture/blob/master/gaps/gap-35_gpu_pci_capability/gap-35_gpu_pci_capability.md).

It supports Nvidia GPUs only. Implementation uses [nvml-wrapper](https://crates.io/crates/nvml-wrapper) to access [NVML](https://developer.nvidia.com/nvidia-management-library-nvml).
