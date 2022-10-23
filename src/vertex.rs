use bytemuck::{Pod, Zeroable};
use dasp::signal::{self as signal, Signal};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    position: [f32; 2],
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

const OUTPUT_START_LEFT: f32 = -1.0;
const OUTPUT_END_LEFT: f32 = 0.0;

const OUTPUT_START_RIGHT: f32 = 0.0;
const OUTPUT_END_RIGHT: f32 = 1.0;

struct Sample {
    left: i32,
    right: i32,
}

pub fn generate_vertexes(ring_buffer: &[i32], channels: i16) -> Vec<Vec<Vertex>> {
    let mut return_vec: Vec<Vec<Vertex>> = vec![];

    match channels {
        1 => {
            let signal: Vec<i32> = signal::from_iter(ring_buffer.iter().cloned())
                .until_exhausted()
                .map(|sample| sample)
                .collect();

            let mut mono_return: Vec<Vertex> = vec![];

            for (i, s) in signal.iter().enumerate() {
                let frac: f32 = signal.len() as f32 / (i as f32 + 1.0);

                let x: f32 = (2.0 / frac) - 1.0;

                let mono_chan = ((*s as f32 - INPUT_START) / (INPUT_END - INPUT_START))
                    * (OUTPUT_END - OUTPUT_START)
                    + OUTPUT_START;

                mono_return.push(Vertex {
                    position: [x, mono_chan],
                    color: [1.0, 1.0, 1.0],
                });
            }

            return_vec.push(mono_return);
            return_vec
        }
        2 => {
            let signal: Vec<Sample> =
                signal::from_interleaved_samples_iter::<_, [i32; 2]>(ring_buffer.iter().cloned())
                    .until_exhausted()
                    .map(|[left, right]| Sample { left, right })
                    .collect();

            let mut left_vec: Vec<Vertex> = vec![];
            let mut right_vec: Vec<Vertex> = vec![];

            for (i, s) in signal.iter().enumerate() {
                let frac: f32 = signal.len() as f32 / (i as f32 + 1.0);

                let x: f32 = (2.0 / frac) - 1.0;

                let left = ((s.left as f32 - INPUT_START) / (INPUT_END - INPUT_START))
                    * (OUTPUT_END_LEFT - OUTPUT_START_LEFT)
                    + OUTPUT_START_LEFT;

                let right = ((s.right as f32 - INPUT_START) / (INPUT_END - INPUT_START))
                    * (OUTPUT_END_RIGHT - OUTPUT_START_RIGHT)
                    + OUTPUT_START_RIGHT;

                left_vec.push(Vertex {
                    position: [x, left],
                    color: [1.0, 1.0, 1.0],
                });

                right_vec.push(Vertex {
                    position: [x, right],
                    color: [1.0, 0.0, 1.0],
                });
            }

            return_vec.push(left_vec);
            return_vec.push(right_vec);

            return_vec
        }
        _ => return_vec,
    }
}
