//! The floor of GPU memory: a Vulkan device through wgpu with nothing on it, held for 30 s so
//! `spike/measure.sh bare` can read it.

fn main() {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::VULKAN;
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("{:?}", adapter.get_info());
    // NODEV=1: stop at the adapter, to tell what opening the device adds.
    if std::env::var_os("NODEV").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(30));
        return;
    }
    let hints = match std::env::var("HINTS").as_deref() {
        Ok("usage") => wgpu::MemoryHints::MemoryUsage,
        _ => wgpu::MemoryHints::Performance,
    };
    let (_device, _queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        memory_hints: hints,
        ..Default::default()
    }))
    .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}
