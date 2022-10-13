use bytemuck::{Pod, Zeroable};
use dasp::signal::{self as signal, Signal};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

const INPUT_START: f32 = -32000.0;
const INPUT_END: f32 = 32000.0;
const OUTPUT_START: f32 = -1.0;
const OUTPUT_END: f32 = 1.0;

pub fn generate_vertexes(ring_buffer: &[i32]) -> Vec<Vertex> {
    let mut count = 0;
    let signal: Vec<Vertex> =
        signal::from_interleaved_samples_iter::<_, [i32; 2]>(ring_buffer.iter().cloned())
            .until_exhausted()
            .map(|[l, r]| {
                let left = ((l as f32 - INPUT_START) / (INPUT_END - INPUT_START))
                    * (OUTPUT_END - OUTPUT_START)
                    + OUTPUT_START;

                let right = ((r as f32 - INPUT_START) / (INPUT_END - INPUT_START))
                    * (OUTPUT_END - OUTPUT_START)
                    + OUTPUT_START;

                count += 1;

                Vertex {
                    position: [left, right, 0.0],
                    color: [0.0, 0.0, 0.0],
                }
            })
            .collect();

    signal
}
