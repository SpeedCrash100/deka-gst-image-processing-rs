/// Gets backend from environment variable WGPU_BACKEND.
pub fn backend() -> wgpu::Backends {
    let env_backend = std::env::var("WGPU_BACKEND").ok().map(|x| x.to_lowercase());
    let Some(env_backend) = env_backend else {
        return wgpu::Backends::all();
    };

    if env_backend == "vulkan" {
        wgpu::Backends::VULKAN
    } else if env_backend == "metal" {
        wgpu::Backends::METAL
    } else if env_backend == "dx12" {
        wgpu::Backends::DX12
    } else if env_backend == "gl" {
        wgpu::Backends::GL
    } else {
        panic!("Invalid backend: {}", env_backend)
    }
}
