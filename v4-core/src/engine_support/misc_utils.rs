use wgpu::{Buffer, Device, Queue, util::DeviceExt};

pub fn update_buffer(
    buf: &mut Buffer,
    data: &[u8],
    device: &Device,
    queue: &Queue,
    label: Option<&str>,
) {
    if data.len() as u64 <= buf.size() {
        queue.write_buffer(buf, 0, data);
    } else {
        *buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label,
            contents: data,
            usage: buf.usage(),
        })
    }
}
