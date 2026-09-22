use wgpu::{Buffer, Device, Queue, util::DeviceExt};

/// Updates the specified buffer with new data. Returns `true` if a the new data exceeds the
/// buffer's capacity and a new buffer is created.
pub fn update_buffer(
    buf: &mut Buffer,
    data: &[u8],
    device: &Device,
    queue: &Queue,
    label: Option<&str>,
) -> bool {
    if data.len() as u64 <= buf.size() {
        queue.write_buffer(buf, 0, data);
        false
    } else {
        *buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label,
            contents: data,
            usage: buf.usage(),
        });

        true
    }
}
